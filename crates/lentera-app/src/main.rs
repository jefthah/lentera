//! Lentera dengan tampilan Slint untuk Windows dan macOS.
//!
//! Data, susunan ibadah, dan penyimpanan ada di `lentera-core`. Berkas ini
//! menghubungkan data itu ke tampilan di `ui/app.slint`: jendela operator dan
//! jendela proyektor.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod update;

use std::{
    cell::RefCell,
    collections::HashSet,
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use lentera_core::bible::{self, Bible, ImportedBible, Lookup, Packing, Translation};
use lentera_core::easyworship;
use lentera_core::{
    Cursor, DEFAULT_PROFILE_NAME, DELETED_PROFILES_DIR, DataLock, Item, Kind, LOCK_FILE, Library,
    Opened, Slide, SongImport, Step, Store, default_data_dir, profiles, remove_profile,
    rename_profile, split_slides,
};
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use slint::winit_030::{WinitWindowAccessor, winit};
use slint::{
    CloseRequestResponse, ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel,
};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::monitor::MonitorHandle;
use winit::window::Fullscreen;

slint::include_modules!();

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SAVE_DELAY: Duration = Duration::from_millis(1500);
const SAVED: &str = "Tersimpan otomatis";
/// Jumlah warna lingkaran profil (`Theme.avatar` di app.slint).
const PROFILE_TONES: u32 = 6;

struct Args {
    data_dir: Option<PathBuf>,
    update_source: Option<String>,
    smoke: bool,
    /// Argumen asli, dipakai lagi saat Lentera dibuka ulang setelah pembaruan.
    raw: Vec<OsString>,
}

fn parse_args() -> Args {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut args = Args {
        data_dir: None,
        update_source: None,
        smoke: false,
        raw: raw.clone(),
    };
    let mut items = raw.iter();
    while let Some(arg) = items.next() {
        match arg.to_str() {
            Some("--data-dir") => args.data_dir = items.next().map(PathBuf::from),
            Some("--update-source") => {
                args.update_source = items.next().map(|v| v.to_string_lossy().into_owned())
            }
            Some("--ui-smoke-test") => args.smoke = true,
            _ => {}
        }
    }
    args
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Song => "Lirik",
        Kind::Verse => "Ayat",
    }
}

fn item_meta(item: &Item) -> String {
    format!("{} · {} slide", kind_label(item.kind), item.slides().len())
}

fn tile(text: &str) -> SlideTile {
    SlideTile {
        text: text.into(),
        first: text.lines().next().unwrap_or_default().into(),
        lines: text.lines().count().max(1) as i32,
        chars: text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as i32,
    }
}

fn tiles(slides: &[String]) -> Vec<SlideTile> {
    slides.iter().map(|s| tile(s)).collect()
}

/// Huruf pertama nama profil, untuk lingkaran inisial.
fn profile_initial(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map_or_else(|| "?".into(), |c| c.to_uppercase().collect())
}

/// Warna lingkaran profil. Diambil dari id, jadi tetap sama walau namanya diganti.
fn profile_tone(id: &str) -> i32 {
    let hash = id.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    (hash % PROFILE_TONES) as i32
}

/// Angka dengan titik ribuan: 31102 → "31.102".
fn count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Awal teks sampai `max` huruf, dipotong di spasi dan diberi "…".
fn snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', '.', ';', ':']))
}

/// Usulan singkatan terjemahan dari nama berkas: "TB.xml" → "TB",
/// "Alkitab BIMK.xml" → "BIMK", "terjemahan-baru.xml" → "TB".
fn suggest_code(stem: &str) -> String {
    let words: Vec<&str> = stem
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let upper = words.iter().find(|w| {
        w.chars().count() <= 6 && w.chars().all(|c| c.is_uppercase() || c.is_ascii_digit())
    });
    match (upper, words.as_slice()) {
        (Some(word), _) => (*word).to_owned(),
        (None, [word]) if word.chars().count() <= 8 => word.to_uppercase(),
        _ => words
            .iter()
            .filter_map(|w| w.chars().next())
            .take(8)
            .collect::<String>()
            .to_uppercase(),
    }
}

fn ui_id(id: u64) -> i32 {
    i32::try_from(id).unwrap_or(-1)
}

fn ui_index(index: Option<usize>) -> i32 {
    index.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1)
}

fn dialog(
    parent: Option<&slint::Window>,
    level: MessageLevel,
    title: &str,
    text: &str,
    buttons: MessageButtons,
) -> MessageDialogResult {
    let mut message = MessageDialog::new()
        .set_level(level)
        .set_title(title)
        .set_description(text)
        .set_buttons(buttons);
    if let Some(window) = parent {
        message = message.set_parent(&window.window_handle());
    }
    message.show()
}

struct State {
    library: Library,
    /// Basis data profil yang sedang dibuka.
    store: Store,
    /// Materi yang terbuka di tengah (dan di editor).
    selected: Option<u64>,
    /// Baris susunan yang terpilih, bila materi dibuka dari susunan.
    selected_service: Option<usize>,
    /// Slide pratinjau.
    page: usize,
    /// Salinan slide yang sedang tayang; mengedit materi tidak mengubahnya.
    live: Option<Slide>,
    live_service: Option<usize>,
    blackout: bool,
    fullscreen: bool,
    output_visible: bool,
    /// Editor punya perubahan yang belum disimpan.
    dirty: bool,
    editing: bool,
    /// Hasil pencarian pustaka yang sedang tampil.
    filtered: Vec<u64>,
}

/// Tab Alkitab.
#[derive(Default)]
struct BibleState {
    /// `alkitab.db`, dibuka saat tab Alkitab pertama kali dipakai.
    db: Option<Bible>,
    translations: Vec<Translation>,
    /// Hasil referensi terakhir yang berhasil.
    lookup: Option<Lookup>,
    /// Baris daftar ayat tempat Shift+klik dimulai.
    anchor: Option<usize>,
    /// Berkas yang sudah dibaca dan menunggu disimpan, beserta nama berkasnya.
    pending: Option<(ImportedBible, String)>,
    scroll_serial: i32,
}

struct Smoke {
    step: u32,
    /// Berapa kali langkah ini sudah menunggu (lihat `App::retry`).
    waits: u32,
    checks: Vec<(String, bool)>,
    report: PathBuf,
}

/// Batas tunggu satu langkah uji, dalam detak 450 ms (sekitar 5 detik).
const SMOKE_MAX_WAITS: u32 = 12;

struct App {
    ui: MainWindow,
    output: OutputWindow,
    state: RefCell<State>,
    library_rows: Rc<VecModel<LibraryRow>>,
    service_rows: Rc<VecModel<ServiceRow>>,
    slide_tiles: Rc<VecModel<SlideTile>>,
    editor_tiles: Rc<VecModel<SlideTile>>,
    profile_cards: Rc<VecModel<ProfileCard>>,
    verse_rows: Rc<VecModel<VerseRow>>,
    bible: RefCell<BibleState>,
    /// Folder data (berisi semua profil).
    data_dir: PathBuf,
    monitors: RefCell<Vec<MonitorHandle>>,
    save_timer: Timer,
    status_timer: Timer,
    smoke_timer: Timer,
    smoke: RefCell<Option<Smoke>>,
    _lock: DataLock,
}

/// Menghubungkan callback Slint ke metode `App` lewat referensi lemah, supaya
/// `App` bisa dilepas saat aplikasi ditutup.
macro_rules! connect {
    ($app:ident . $comp:ident . $setter:ident, |$a:ident $(, $arg:ident)*| $body:expr) => {{
        let weak = Rc::downgrade(&$app);
        $app.$comp.$setter(move |$($arg),*| {
            if let Some($a) = weak.upgrade() {
                $body;
            }
        });
    }};
}

impl App {
    fn new(
        library: Library,
        store: Store,
        lock: DataLock,
        data_dir: PathBuf,
    ) -> Result<Rc<Self>, String> {
        let ui = MainWindow::new().map_err(|e| e.to_string())?;
        let output = OutputWindow::new().map_err(|e| e.to_string())?;
        let background = library.background as i32;
        let app = Rc::new(Self {
            ui,
            output,
            state: RefCell::new(State {
                library,
                store,
                selected: None,
                selected_service: None,
                page: 0,
                live: None,
                live_service: None,
                blackout: false,
                fullscreen: false,
                output_visible: false,
                dirty: false,
                editing: false,
                filtered: Vec::new(),
            }),
            library_rows: Rc::new(VecModel::default()),
            service_rows: Rc::new(VecModel::default()),
            slide_tiles: Rc::new(VecModel::default()),
            editor_tiles: Rc::new(VecModel::default()),
            profile_cards: Rc::new(VecModel::default()),
            verse_rows: Rc::new(VecModel::default()),
            bible: RefCell::default(),
            data_dir,
            monitors: RefCell::default(),
            save_timer: Timer::default(),
            status_timer: Timer::default(),
            smoke_timer: Timer::default(),
            smoke: RefCell::default(),
            _lock: lock,
        });
        let ui = &app.ui;
        ui.set_version(VERSION.into());
        ui.set_library(ModelRc::from(app.library_rows.clone()));
        ui.set_service(ModelRc::from(app.service_rows.clone()));
        ui.set_slides(ModelRc::from(app.slide_tiles.clone()));
        ui.set_editor_slides(ModelRc::from(app.editor_tiles.clone()));
        ui.set_profiles(ModelRc::from(app.profile_cards.clone()));
        ui.set_bible_verses(ModelRc::from(app.verse_rows.clone()));
        ui.set_bg_index(background);
        app.output.set_bg_index(background);
        Ok(app)
    }

    fn connect(self: &Rc<Self>) {
        let app = self.clone();
        connect!(app.ui.on_search_changed, |a| a.refresh_library());
        connect!(app.ui.on_filter_changed, |a, _filter| a.refresh_library());
        connect!(app.ui.on_select_library, |a, id| a.select_library(id));
        connect!(app.ui.on_activate_library, |a, id| {
            a.select_library(id);
            a.go_live()
        });
        connect!(app.ui.on_add_to_service, |a, id| a.add_to_service(id));
        connect!(app.ui.on_select_service, |a, index| a.select_service(index));
        connect!(app.ui.on_activate_service, |a, index| {
            a.select_service(index);
            a.go_live()
        });
        connect!(app.ui.on_move_service, |a, index, delta| a
            .move_service(index, delta));
        connect!(app.ui.on_remove_service, |a, index| a.remove_service(index));
        connect!(app.ui.on_select_slide, |a, page| a.select_slide(page));
        connect!(app.ui.on_activate_slide, |a, page| {
            a.select_slide(page);
            a.go_live()
        });
        connect!(app.ui.on_go_live, |a| a.go_live());
        connect!(app.ui.on_navigate, |a, forward| a.navigate(forward));
        connect!(app.ui.on_toggle_blackout, |a| a.toggle_blackout());
        connect!(app.ui.on_toggle_fullscreen, |a| a.toggle_fullscreen());
        connect!(app.ui.on_toggle_projector, |a| a.toggle_projector());
        connect!(app.ui.on_escape, |a| a.escape());
        connect!(app.ui.on_monitor_changed, |a, index| a
            .monitor_changed(index));
        connect!(app.ui.on_background_changed, |a, index| a
            .background_changed(index));
        connect!(app.ui.on_new_item, |a| a.new_item());
        connect!(app.ui.on_edit_current, |a| a.edit_current());
        connect!(app.ui.on_close_editor, |a| a.close_editor());
        connect!(app.ui.on_editor_edited, |a| a.editor_edited());
        connect!(app.ui.on_set_song, |a, song| a.set_song(song));
        connect!(app.ui.on_add_current_to_service, |a| a
            .add_current_to_service());
        connect!(app.ui.on_delete_current, |a| a.delete_current());
        connect!(app.ui.on_save_now, |a| a.save_now());
        connect!(app.ui.on_import_lyrics, |a| a.import(false));
        connect!(app.ui.on_import_easyworship, |a| a.import_easyworship());
        connect!(app.ui.on_import_verses, |a| a.import(true));
        connect!(app.ui.on_update_now, |a| a.update_now());
        connect!(app.ui.on_show_profiles, |a| a.show_profiles());
        connect!(app.ui.on_close_profiles, |a| a.close_profiles());
        connect!(app.ui.on_choose_profile, |a, id| a.choose_profile(&id));
        connect!(app.ui.on_submit_profile_form, |a| a.submit_profile_form());
        connect!(app.ui.on_delete_profile, |a, id| a.delete_profile(&id));
        connect!(app.ui.on_left_tab_changed, |a, tab| a.left_tab_changed(tab));
        connect!(app.ui.on_bible_translation_changed, |a, _index| a
            .bible_translation_changed());
        connect!(app.ui.on_bible_ref_edited, |a| a.bible_lookup(true));
        connect!(app.ui.on_bible_go_live, |a| a.bible_go_live());
        connect!(app.ui.on_bible_verse_clicked, |a, index, shift| a
            .bible_verse_clicked(index, shift));
        connect!(app.ui.on_bible_verse_activated, |a, index| {
            a.bible_verse_clicked(index, false);
            a.go_live()
        });
        connect!(app.ui.on_bible_add_to_service, |a| a.bible_add_to_service());
        connect!(app.ui.on_bible_packing_changed, |a| a.bible_lookup(false));
        connect!(app.ui.on_bible_import, |a| a.bible_import());
        connect!(app.ui.on_bible_import_save, |a| a.bible_import_save());
        connect!(app.ui.on_bible_import_cancel, |a| a.bible_import_cancel());
        connect!(app.ui.on_bible_remove_translation, |a| a
            .bible_remove_translation());

        connect!(app.output.on_go_live, |a| a.go_live());
        connect!(app.output.on_navigate, |a, forward| a.navigate(forward));
        connect!(app.output.on_toggle_blackout, |a| a.toggle_blackout());
        connect!(app.output.on_toggle_fullscreen, |a| a.toggle_fullscreen());
        connect!(app.output.on_escape, |a| a.escape());

        let weak = Rc::downgrade(self);
        self.ui.window().on_close_requested(move || {
            if let Some(a) = weak.upgrade() {
                a.close();
            }
            CloseRequestResponse::HideWindow
        });
        let weak = Rc::downgrade(self);
        self.output.window().on_close_requested(move || {
            if let Some(a) = weak.upgrade() {
                a.output_closed();
            }
            CloseRequestResponse::HideWindow
        });
    }

    // ----- Tampilan -----

    fn status(&self, text: &str, ok: bool) {
        self.ui.set_status(text.into());
        self.ui.set_status_ok(ok);
        let ui = self.ui.as_weak();
        self.status_timer
            .start(TimerMode::SingleShot, Duration::from_secs(8), move || {
                if let Some(ui) = ui.upgrade() {
                    ui.set_status(SAVED.into());
                    ui.set_status_ok(true);
                }
            });
    }

    fn refresh_library(&self) {
        let query = self.ui.get_query();
        let filter = match self.ui.get_filter() {
            1 => Some(Kind::Song),
            2 => Some(Kind::Verse),
            _ => None,
        };
        let mut st = self.state.borrow_mut();
        let rows: Vec<LibraryRow> = st
            .library
            .search(filter, query.as_str())
            .into_iter()
            .map(|item| LibraryRow {
                id: ui_id(item.id),
                title: item.title.as_str().into(),
                meta: item_meta(item).into(),
                song: item.kind == Kind::Song,
            })
            .collect();
        let total = st.library.items.len();
        st.filtered = rows.iter().map(|r| r.id as u64).collect();
        drop(st);
        let detail = if rows.len() < total {
            format!("{} dari {total}", rows.len())
        } else {
            format!("{total} materi")
        };
        self.library_rows.set_vec(rows);
        self.ui.set_library_detail(detail.into());
    }

    fn refresh_service(&self) {
        let st = self.state.borrow();
        let rows: Vec<ServiceRow> = st
            .library
            .service
            .iter()
            .filter_map(|id| st.library.item(*id))
            .map(|item| ServiceRow {
                id: ui_id(item.id),
                title: item.title.as_str().into(),
                meta: item_meta(item).into(),
                song: item.kind == Kind::Song,
            })
            .collect();
        let selected = ui_index(st.selected_service);
        let live = ui_index(st.live_service);
        drop(st);
        self.service_rows.set_vec(rows);
        self.ui.set_selected_service(selected);
        self.ui.set_live_service(live);
    }

    /// Materi terpilih: judul, semua slide, dan pratinjau.
    fn refresh_current(&self) {
        let st = self.state.borrow();
        let current = st
            .selected
            .and_then(|id| st.library.item(id))
            .map(|item| (item.id, item.title.clone(), item.kind, item.slides()));
        drop(st);
        let Some((id, title, kind, slides)) = current else {
            self.ui.set_has_current(false);
            self.ui.set_selected_id(-1);
            self.slide_tiles.set_vec(Vec::new());
            self.refresh_preview();
            return;
        };
        self.ui.set_has_current(true);
        self.ui.set_selected_id(ui_id(id));
        self.ui.set_current_title(title.into());
        let meta = if Library::is_scratch(id) {
            format!("Alkitab · {} slide · belum di pustaka", slides.len())
        } else {
            format!("{} · {} slide", kind_label(kind), slides.len())
        };
        self.ui.set_current_meta(meta.into());
        self.slide_tiles.set_vec(tiles(&slides));
        self.refresh_preview();
    }

    fn refresh_preview(&self) {
        let mut st = self.state.borrow_mut();
        let current = st
            .selected
            .and_then(|id| st.library.item(id))
            .map(|item| (item.title.clone(), item.slides()));
        let Some((title, slides)) = current.filter(|(_, slides)| !slides.is_empty()) else {
            drop(st);
            self.ui.set_has_preview(false);
            self.ui.set_preview(SlideTile::default());
            self.ui.set_preview_meta(SharedString::new());
            self.ui.set_preview_page(-1);
            return;
        };
        st.page = st.page.min(slides.len() - 1);
        let page = st.page;
        drop(st);
        self.ui.set_has_preview(true);
        self.ui.set_preview(tile(&slides[page]));
        self.ui
            .set_preview_meta(format!("{title} · {}/{}", page + 1, slides.len()).into());
        self.ui.set_preview_page(page as i32);
    }

    fn refresh_live(&self) {
        let st = self.state.borrow();
        let live_tile = st.live.as_ref().map(|s| tile(&s.text)).unwrap_or_default();
        let meta = match (&st.live, st.blackout) {
            (_, true) => "Layar digelapkan".to_owned(),
            (Some(s), false) => format!("{} · {}/{}", s.title, s.page + 1, s.total),
            (None, false) => "Belum ada yang tayang".to_owned(),
        };
        let has_live = st.live.is_some();
        let item = st.live.as_ref().map_or(-1, |s| ui_id(s.item));
        let page = st.live.as_ref().map_or(-1, |s| s.page as i32);
        let service = ui_index(st.live_service);
        let (blackout, visible, fullscreen) = (st.blackout, st.output_visible, st.fullscreen);
        drop(st);
        self.ui.set_live(live_tile.clone());
        self.ui.set_has_live(has_live);
        self.ui.set_live_meta(meta.into());
        self.ui.set_live_item_id(item);
        self.ui.set_live_page(page);
        self.ui.set_live_service(service);
        self.ui.set_blackout(blackout);
        self.ui.set_output_visible(visible);
        self.ui.set_fullscreen(fullscreen);
        self.output.set_tile(live_tile);
        self.output.set_has_slide(has_live);
        self.output.set_dark(blackout);
        self.output.set_fullscreen(fullscreen);
    }

    fn load_editor(&self) {
        let st = self.state.borrow();
        let item = st.selected.and_then(|id| st.library.item(id)).cloned();
        drop(st);
        match item {
            Some(item) => {
                self.editor_tiles.set_vec(tiles(&item.slides()));
                self.ui.set_editor_song(item.kind == Kind::Song);
                self.ui.set_editor_title(item.title.into());
                self.ui.set_editor_body(item.content.into());
            }
            None => {
                self.editor_tiles.set_vec(Vec::new());
                self.ui.set_editor_title(SharedString::new());
                self.ui.set_editor_body(SharedString::new());
            }
        }
    }

    // ----- Data -----

    fn persist(&self) -> bool {
        let result = {
            let st = self.state.borrow();
            st.store.save(&st.library)
        };
        match result {
            Ok(()) => true,
            Err(e) => {
                dialog(
                    Some(self.ui.window()),
                    MessageLevel::Error,
                    "Penyimpanan gagal",
                    &format!("Perubahan belum tersimpan.\n{e}"),
                    MessageButtons::Ok,
                );
                false
            }
        }
    }

    /// Menyimpan isi editor bila berubah. `false` bila penyimpanan gagal.
    fn commit_editor(&self) -> bool {
        self.save_timer.stop();
        let id = {
            let st = self.state.borrow();
            if !st.dirty {
                return true;
            }
            st.selected
        };
        if let Some(id) = id {
            let title = self.ui.get_editor_title();
            let body = self.ui.get_editor_body();
            self.state.borrow_mut().library.update(id, &title, &body);
        }
        if !self.persist() {
            return false;
        }
        self.state.borrow_mut().dirty = false;
        self.refresh_library();
        self.refresh_service();
        self.refresh_current();
        true
    }

    fn select(&self, id: u64, service: Option<usize>) {
        {
            let mut st = self.state.borrow_mut();
            st.selected = Some(id);
            st.selected_service = service;
            st.page = 0;
            st.dirty = false;
        }
        self.ui.set_selected_service(ui_index(service));
        self.load_editor();
        self.refresh_current();
    }

    fn select_library(&self, id: i32) {
        if id < 0 || !self.commit_editor() {
            return;
        }
        self.select(id as u64, None);
    }

    fn select_service(&self, index: i32) {
        if !self.commit_editor() {
            return;
        }
        let id = usize::try_from(index)
            .ok()
            .and_then(|i| self.state.borrow().library.service.get(i).copied());
        if let Some(id) = id {
            self.select(id, Some(index as usize));
        }
    }

    fn select_slide(&self, page: i32) {
        if let Ok(page) = usize::try_from(page) {
            self.state.borrow_mut().page = page;
            self.refresh_preview();
        }
    }

    fn add_to_service(&self, id: i32) {
        if id < 0 || !self.commit_editor() {
            return;
        }
        let Some(id) = self.materialize(id as u64) else {
            return;
        };
        let title = {
            let mut st = self.state.borrow_mut();
            let Some(title) = st.library.item(id).map(|i| i.title.clone()) else {
                return;
            };
            st.library.service.push(id);
            title
        };
        self.refresh_service();
        if self.persist() {
            self.status(&format!("“{title}” ditambahkan ke susunan ibadah."), true);
        }
    }

    fn add_current_to_service(&self) {
        let selected = self.state.borrow().selected;
        if let Some(id) = selected {
            self.add_to_service(ui_id(id));
        }
    }

    fn move_service(&self, index: i32, delta: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        {
            let mut st = self.state.borrow_mut();
            let len = st.library.service.len();
            if index >= len {
                return;
            }
            let target = if delta < 0 {
                index.saturating_sub(1)
            } else {
                (index + 1).min(len - 1)
            };
            if target == index {
                return;
            }
            st.library.service.swap(index, target);
            let fix = |p: Option<usize>| {
                p.map(|p| match p {
                    p if p == index => target,
                    p if p == target => index,
                    p => p,
                })
            };
            st.live_service = fix(st.live_service);
            st.selected_service = fix(st.selected_service);
        }
        self.refresh_service();
        self.persist();
    }

    fn remove_service(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        {
            let mut st = self.state.borrow_mut();
            if index >= st.library.service.len() {
                return;
            }
            st.library.service.remove(index);
            let fix = |p: Option<usize>| {
                p.and_then(|p| match p {
                    p if p == index => None,
                    p if p > index => Some(p - 1),
                    p => Some(p),
                })
            };
            st.live_service = fix(st.live_service);
            st.selected_service = fix(st.selected_service);
        }
        self.refresh_service();
        self.persist();
    }

    fn new_item(&self) {
        if !self.commit_editor() {
            return;
        }
        let kind = if self.ui.get_filter() == 2 {
            Kind::Verse
        } else {
            Kind::Song
        };
        let title = if kind == Kind::Verse {
            "Ayat baru"
        } else {
            "Lagu baru"
        };
        let id = self
            .state
            .borrow_mut()
            .library
            .add(kind, title.into(), String::new());
        self.ui.set_left_tab(0);
        self.ui.set_query(SharedString::new());
        self.refresh_library();
        self.select(id, None);
        self.persist();
        self.open_editor(true);
    }

    fn open_editor(&self, focus_title: bool) {
        self.state.borrow_mut().editing = true;
        self.ui.set_focus_title(focus_title);
        self.ui.set_editing(true);
    }

    fn edit_current(&self) {
        let Some(id) = self.state.borrow().selected else {
            self.status("Pilih materi yang ingin diedit.", false);
            return;
        };
        // Ayat Alkitab disimpan dulu ke pustaka, baru bisa diedit.
        if Library::is_scratch(id) && (self.materialize(id).is_none() || !self.persist()) {
            return;
        }
        self.load_editor();
        self.open_editor(false);
    }

    fn close_editor(&self) {
        if !self.commit_editor() {
            return;
        }
        self.state.borrow_mut().editing = false;
        self.ui.set_editing(false);
        self.refresh_current();
    }

    fn editor_edited(self: &Rc<Self>) {
        self.state.borrow_mut().dirty = true;
        let body = self.ui.get_editor_body();
        self.editor_tiles.set_vec(tiles(&split_slides(&body)));
        let weak = Rc::downgrade(self);
        self.save_timer
            .start(TimerMode::SingleShot, SAVE_DELAY, move || {
                if let Some(app) = weak.upgrade() {
                    app.commit_editor();
                }
            });
    }

    fn set_song(&self, song: bool) {
        if !self.commit_editor() {
            return;
        }
        let kind = if song { Kind::Song } else { Kind::Verse };
        {
            let mut st = self.state.borrow_mut();
            let Some(id) = st.selected else {
                return;
            };
            if let Some(item) = st.library.items.iter_mut().find(|i| i.id == id) {
                item.kind = kind;
            }
        }
        self.ui.set_editor_song(song);
        self.persist();
        self.refresh_library();
        self.refresh_service();
        self.refresh_current();
    }

    fn delete_current(&self) {
        let current = {
            let st = self.state.borrow();
            st.selected
                .filter(|id| !Library::is_scratch(*id))
                .and_then(|id| st.library.item(id))
                .map(|i| (i.id, i.title.clone()))
        };
        let Some((id, title)) = current else {
            return;
        };
        let answer = dialog(
            Some(self.ui.window()),
            MessageLevel::Warning,
            "Hapus materi",
            &format!("Hapus “{title}” dari pustaka dan susunan ibadah?"),
            MessageButtons::YesNo,
        );
        if answer != MessageDialogResult::Yes {
            return;
        }
        self.save_timer.stop();
        {
            let mut st = self.state.borrow_mut();
            st.library.remove(id);
            st.selected = None;
            st.selected_service = None;
            st.dirty = false;
            st.editing = false;
            let live_item = st.live.as_ref().map(|s| s.item);
            st.live_service = live_item.and_then(|item| st.library.service_position(item, None));
        }
        self.ui.set_editing(false);
        self.refresh_library();
        self.refresh_service();
        self.refresh_current();
        self.load_editor();
        self.refresh_live();
        if self.persist() {
            self.status(&format!("“{title}” dihapus."), true);
        }
    }

    fn save_now(&self) {
        let dirty = self.state.borrow().dirty;
        let saved = if dirty {
            self.commit_editor()
        } else {
            self.persist()
        };
        if saved {
            self.status("Materi dan susunan tersimpan.", true);
        }
    }

    /// Membuka dialog pilih berkas, lalu menjalankan `then` dengan berkas yang
    /// dipilih sesaat kemudian, setelah callback ini selesai.
    ///
    /// Setelah dialog berkas tertutup, jendela Lentera perlu digambar ulang.
    /// Selama callback Slint masih berjalan, winit terus mengulang permintaan
    /// gambar ulang itu, jadi antrean pesan Windows tidak pernah kosong.
    /// Dialog pesan (MessageBox) baru tampil saat antrean kosong, sehingga
    /// dialog konfirmasi atau galat sesudahnya tidak pernah muncul dan Lentera
    /// tampak macet.
    fn pick_file_then(
        self: &Rc<Self>,
        picker: rfd::FileDialog,
        then: impl FnOnce(&Self, &Path) + 'static,
    ) {
        let Some(path) = picker
            .set_parent(&self.ui.window().window_handle())
            .pick_file()
        else {
            return;
        };
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::from_millis(50), move || {
            if let Some(app) = weak.upgrade() {
                then(&app, &path);
            }
        });
    }

    fn import(self: &Rc<Self>, bible: bool) {
        if !self.commit_editor() {
            return;
        }
        let picker = if bible {
            rfd::FileDialog::new()
                .set_title("Impor ayat Alkitab")
                .add_filter("Ayat TSV UTF-8", &["tsv"])
        } else {
            rfd::FileDialog::new()
                .set_title("Impor lirik lagu")
                .add_filter("Lirik UTF-8", &["txt"])
        };
        self.pick_file_then(
            picker.add_filter("Semua berkas", &["*"]),
            move |app, path| app.import_file(bible, path),
        );
    }

    fn import_file(&self, bible: bool, path: &Path) {
        let result = {
            let mut st = self.state.borrow_mut();
            if bible {
                st.library
                    .import_verses_file(path)
                    .map(|count| (None, format!("{count} ayat baru diimpor.")))
            } else {
                st.library.import_song_file(path).map(|id| {
                    (
                        Some(id),
                        "Lirik diimpor. Baris kosong memisahkan slide.".to_owned(),
                    )
                })
            }
        };
        match result {
            Ok((id, message)) => {
                self.refresh_library();
                if let Some(id) = id {
                    self.select(id, None);
                }
                if self.persist() {
                    self.status(&message, true);
                }
            }
            Err(e) => {
                dialog(
                    Some(self.ui.window()),
                    MessageLevel::Error,
                    "Impor gagal",
                    &e,
                    MessageButtons::Ok,
                );
            }
        }
    }

    /// Mengimpor semua lagu dari database EasyWorship sekaligus, setelah
    /// jumlahnya ditunjukkan dan disetujui. Berkas EasyWorship hanya dibaca.
    fn import_easyworship(self: &Rc<Self>) {
        if !self.commit_editor() {
            return;
        }
        let mut picker = rfd::FileDialog::new()
            .set_title("Pilih database lagu EasyWorship (Songs.db)")
            .add_filter("Database lagu EasyWorship (Songs.db)", &["db"])
            .add_filter("Semua berkas", &["*"]);
        if let Some(dir) = easyworship::song_locations()
            .first()
            .and_then(|file| file.parent())
        {
            picker = picker.set_directory(dir);
        }
        self.pick_file_then(picker, |app, path| app.import_easyworship_file(path));
    }

    /// Membaca database lagu EasyWorship yang dipilih, menunjukkan jumlahnya,
    /// lalu mengimpor setelah disetujui.
    fn import_easyworship_file(&self, path: &Path) {
        let database = match easyworship::read_songs(path) {
            Ok(database) => database,
            Err(e) => {
                dialog(
                    Some(self.ui.window()),
                    MessageLevel::Error,
                    "Impor EasyWorship gagal",
                    &e,
                    MessageButtons::Ok,
                );
                return;
            }
        };
        let found = database.songs.len();
        let (plan, profile) = {
            let st = self.state.borrow();
            let songs = database.songs.into_iter().map(|s| (s.title, s.content));
            (
                st.library.plan_song_import(songs),
                st.store.name().to_owned(),
            )
        };
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        let mut text = format!(
            "{}: {} lagu ditemukan di {file}.\n",
            database.edition.name(),
            count(found)
        );
        if plan.songs.is_empty() {
            text.push_str(&format!(
                "\nSemua lagu itu sudah ada di pustaka profil \u{201c}{profile}\u{201d}."
            ));
            dialog(
                Some(self.ui.window()),
                MessageLevel::Info,
                "Impor lagu EasyWorship",
                &text,
                MessageButtons::Ok,
            );
            return;
        }
        text.push_str(&format!(
            "\n\u{2022} {} lagu baru ditambahkan ke profil \u{201c}{profile}\u{201d}.",
            count(plan.songs.len())
        ));
        if plan.existing > 0 {
            text.push_str(&format!(
                "\n\u{2022} {} lagu sudah ada di pustaka dan dilewati.",
                count(plan.existing)
            ));
        }
        if plan.renamed > 0 {
            text.push_str(&format!(
                "\n\u{2022} {} lagu judulnya sudah dipakai lagu dengan lirik lain, jadi \
                 diberi nomor, misalnya \u{201c}Kasih (2)\u{201d}.",
                count(plan.renamed)
            ));
        }
        if database.empty > 0 {
            text.push_str(&format!(
                "\n\u{2022} {} lagu tanpa lirik dilewati.",
                count(database.empty)
            ));
        }
        text.push_str("\n\nLabel bagian seperti \u{201c}Verse 1\u{201d} dan \u{201c}Chorus\u{201d} tidak ikut. Impor sekarang?");
        let answer = dialog(
            Some(self.ui.window()),
            MessageLevel::Info,
            "Impor lagu EasyWorship",
            &text,
            MessageButtons::YesNo,
        );
        if answer == MessageDialogResult::Yes {
            self.add_imported_songs(plan);
        }
    }

    /// Menambahkan lagu hasil impor ke pustaka, memilih lagu pertama, dan
    /// menyimpan. Mengembalikan jumlah lagu yang ditambahkan.
    fn add_imported_songs(&self, plan: SongImport) -> usize {
        let added = self.state.borrow_mut().library.add_songs(plan);
        self.refresh_library();
        if let Some(&first) = added.first() {
            self.select(first, None);
        }
        if self.persist() {
            self.status(
                &format!("{} lagu EasyWorship diimpor.", count(added.len())),
                true,
            );
        }
        added.len()
    }

    // ----- Alkitab -----

    /// Menyimpan materi sementara (ayat Alkitab) ke pustaka. Bila ayat yang
    /// sama sudah ada di pustaka, materi itu yang dipakai. Mengembalikan id
    /// materi di pustaka; materi biasa dikembalikan apa adanya. Belum disimpan
    /// ke basis data.
    fn materialize(&self, id: u64) -> Option<u64> {
        if !Library::is_scratch(id) {
            return Some(id);
        }
        {
            let mut st = self.state.borrow_mut();
            let scratch = st.library.item(id)?.clone();
            let saved = st
                .library
                .items
                .iter()
                .find(|i| {
                    i.kind == scratch.kind
                        && i.title == scratch.title
                        && i.content == scratch.content
                })
                .map(|i| i.id)
                .unwrap_or_else(|| st.library.add(scratch.kind, scratch.title, scratch.content));
            if st.selected == Some(id) {
                st.selected = Some(saved);
            }
            if let Some(live) = st.live.as_mut().filter(|live| live.item == id) {
                live.item = saved;
            }
            drop(st);
            self.refresh_library();
            self.refresh_current();
            self.refresh_live();
            Some(saved)
        }
    }

    /// Membuka `alkitab.db` bila belum. `false` bila gagal; pesannya sudah tampil.
    fn open_bible(&self) -> bool {
        if self.bible.borrow().db.is_some() {
            return true;
        }
        match Bible::open(&self.data_dir) {
            Ok(db) => {
                self.bible.borrow_mut().db = Some(db);
                self.refresh_translations(None);
                true
            }
            Err(e) => {
                dialog(
                    Some(self.ui.window()),
                    MessageLevel::Error,
                    "Alkitab tidak bisa dibuka",
                    &e,
                    MessageButtons::Ok,
                );
                false
            }
        }
    }

    /// Pilihan terjemahan. `select` (id) dipilih; bila tidak ada, terjemahan
    /// yang terakhir dipakai.
    fn refresh_translations(&self, select: Option<i64>) {
        let mut bible = self.bible.borrow_mut();
        let Some(db) = bible.db.as_ref() else {
            return;
        };
        let list = match db.translations() {
            Ok(list) => list,
            Err(e) => {
                drop(bible);
                self.status(&e, false);
                return;
            }
        };
        let wanted = select.or_else(|| db.last_translation());
        let index = wanted
            .and_then(|id| list.iter().position(|t| t.id == id))
            .unwrap_or(0);
        let labels: Vec<SharedString> = list
            .iter()
            .map(|t| {
                if t.name == t.code {
                    t.code.as_str().into()
                } else {
                    format!("{} · {}", t.code, t.name).into()
                }
            })
            .collect();
        bible.translations = list;
        drop(bible);
        self.ui
            .set_bible_translations(ModelRc::new(VecModel::from(labels)));
        self.ui.set_bible_translation(index as i32);
    }

    fn current_translation(&self) -> Option<Translation> {
        let index = usize::try_from(self.ui.get_bible_translation()).ok()?;
        self.bible.borrow().translations.get(index).cloned()
    }

    fn left_tab_changed(&self, tab: i32) {
        if tab == 1 {
            self.open_bible();
        }
    }

    fn bible_translation_changed(&self) {
        if let Some(translation) = self.current_translation()
            && let Some(db) = self.bible.borrow().db.as_ref()
        {
            let _ = db.set_last_translation(translation.id);
        }
        self.bible_lookup(true);
    }

    /// Mengosongkan hasil referensi di tab Alkitab.
    fn clear_bible_result(&self) {
        {
            let mut bible = self.bible.borrow_mut();
            bible.lookup = None;
            bible.anchor = None;
        }
        self.verse_rows.set_vec(Vec::new());
        self.ui.set_bible_label(SharedString::new());
        self.ui.set_bible_detail(SharedString::new());
        self.ui.set_bible_error(SharedString::new());
        self.ui.set_bible_chapter_title(SharedString::new());
        self.ui.set_bible_has_passage(false);
    }

    /// Mencari referensi di kotak Alkitab dan menampilkan ayatnya di tengah.
    /// `typed`: referensi diketik (bukan dari klik daftar ayat), jadi daftar
    /// digulir ke ayat pertama dan titik awal Shift+klik dipindah ke sana.
    fn bible_lookup(&self, typed: bool) {
        let input = self.ui.get_bible_ref();
        let Some(translation) = self.current_translation() else {
            self.clear_bible_result();
            return;
        };
        if input.trim().is_empty() {
            self.clear_bible_result();
            return;
        }
        let result = {
            let bible = self.bible.borrow();
            let Some(db) = bible.db.as_ref() else {
                return;
            };
            db.lookup(translation.id, &input)
        };
        match result {
            Ok(found) => self.show_lookup(found, &translation, typed),
            // Daftar ayat dan slide sebelumnya tetap tampil selama mengetik.
            Err(e) => {
                self.ui.set_bible_error(e.into());
                self.ui.set_bible_has_passage(false);
            }
        }
    }

    fn show_lookup(&self, found: Lookup, translation: &Translation, typed: bool) {
        let book = found.reference.book;
        let picked: HashSet<(u16, u16)> =
            found.verses.iter().map(|v| (v.chapter, v.verse)).collect();
        let rows: Vec<VerseRow> = found
            .chapter_verses
            .iter()
            .map(|v| VerseRow {
                number: v.number().into(),
                text: snippet(&v.text, 76).into(),
                picked: picked.contains(&(v.chapter, v.verse)),
            })
            .collect();
        let first = rows.iter().position(|r| r.picked);
        let same_chapter = self.bible.borrow().lookup.as_ref().is_some_and(|old| {
            old.reference.book.number == book.number && old.chapter == found.chapter
        });
        self.verse_rows.set_vec(rows);
        self.ui.set_bible_error(SharedString::new());
        self.ui.set_bible_chapter_title(
            format!("Pasal {} dari {}", found.chapter, found.chapters).into(),
        );
        if typed || !same_chapter {
            let mut bible = self.bible.borrow_mut();
            bible.scroll_serial += 1;
            if typed {
                bible.anchor = first;
            }
            self.ui.set_bible_scroll_row(first.unwrap_or(0) as i32);
            self.ui.set_bible_scroll_serial(bible.scroll_serial);
        }
        if found.verses.is_empty() {
            // Baru nama kitab: daftar pasal pertama untuk dipilih.
            self.ui.set_bible_label(book.name.into());
            self.ui
                .set_bible_detail("Ketik pasal dan ayat, atau klik ayat di bawah.".into());
            self.ui.set_bible_has_passage(false);
            self.bible.borrow_mut().lookup = Some(found);
            return;
        }
        let packing = if self.ui.get_bible_one_per_slide() {
            Packing::One
        } else {
            Packing::Auto
        };
        let content = bible::passage_text(&found.verses, packing);
        let title = format!("{} ({})", found.label, translation.code);
        let slides = split_slides(&content).len();
        self.ui.set_bible_label(found.label.as_str().into());
        self.ui.set_bible_detail(
            format!(
                "{} ayat · {slides} slide · Enter: tayangkan",
                found.verses.len()
            )
            .into(),
        );
        self.ui.set_bible_has_passage(true);
        self.bible.borrow_mut().lookup = Some(found);
        self.show_scratch(title, content);
    }

    /// Menampilkan ayat di tengah sebagai materi sementara.
    fn show_scratch(&self, title: String, content: String) {
        if self.state.borrow().editing {
            self.close_editor();
            if self.state.borrow().editing {
                return;
            }
        }
        let id = {
            let mut st = self.state.borrow_mut();
            // Ayat yang sama sudah tampil: pratinjau tetap di slide yang sama.
            let shown = st
                .selected
                .filter(|id| Library::is_scratch(*id))
                .and_then(|id| st.library.item(id))
                .is_some_and(|i| i.title == title && i.content == content);
            if shown {
                return;
            }
            let keep = st
                .live
                .as_ref()
                .map(|s| s.item)
                .filter(|id| Library::is_scratch(*id));
            st.library.set_scratch(Kind::Verse, title, content, keep)
        };
        self.select(id, None);
    }

    /// Enter di kotak referensi: tayangkan ayatnya.
    fn bible_go_live(self: &Rc<Self>) {
        self.bible_lookup(false);
        if self.ui.get_bible_has_passage() {
            self.go_live();
        }
    }

    /// Klik ayat di daftar: pilih ayat itu. Shift+klik: rentang dari ayat
    /// yang terakhir diklik.
    fn bible_verse_clicked(&self, index: i32, shift: bool) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let (text, anchor) = {
            let bible = self.bible.borrow();
            let Some(found) = bible.lookup.as_ref() else {
                return;
            };
            let Some(verse) = found.chapter_verses.get(index) else {
                return;
            };
            let from = bible
                .anchor
                .filter(|_| shift)
                .and_then(|a| found.chapter_verses.get(a))
                .unwrap_or(verse);
            let (first, last) = if from.verse <= verse.verse {
                (from, verse)
            } else {
                (verse, from)
            };
            let short = found.reference.book.short;
            let text = if first.verse == last.last {
                format!("{short} {}:{}", found.chapter, first.verse)
            } else {
                format!("{short} {}:{}-{}", found.chapter, first.verse, last.last)
            };
            let anchor = if shift && bible.anchor.is_some() {
                bible.anchor
            } else {
                Some(index)
            };
            (text, anchor)
        };
        self.bible.borrow_mut().anchor = anchor;
        self.ui.set_bible_ref(text.into());
        self.bible_lookup(false);
    }

    fn bible_add_to_service(&self) {
        self.bible_lookup(false);
        if !self.ui.get_bible_has_passage() {
            return;
        }
        let selected = self.state.borrow().selected;
        if let Some(id) = selected.filter(|id| Library::is_scratch(*id)) {
            self.add_to_service(ui_id(id));
        }
    }

    fn bible_import(self: &Rc<Self>) {
        if !self.commit_editor() || !self.open_bible() {
            return;
        }
        let mut picker = rfd::FileDialog::new()
            .set_title("Impor Alkitab")
            .add_filter(
                "Alkitab (OpenSong / Zefania XML, TSV, EasyWorship)",
                &["xml", "tsv", "txt", "ewb"],
            )
            .add_filter("Semua berkas", &["*"]);
        // Alkitab EasyWorship ada di folder tersembunyi; buka di sana bila ada.
        if let Some(dir) = easyworship::bible_locations().first() {
            picker = picker.set_directory(dir);
        }
        self.pick_file_then(picker, |app, path| {
            if let Err(e) = app.bible_read_file(path) {
                dialog(
                    Some(app.ui.window()),
                    MessageLevel::Error,
                    "Impor Alkitab gagal",
                    &e,
                    MessageButtons::Ok,
                );
            }
        });
    }

    /// Membaca berkas Alkitab lalu membuka formulir impor di tab Alkitab.
    fn bible_read_file(&self, path: &Path) -> Result<(), String> {
        let imported = bible::read_bible_file(path)?;
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let code = imported
            .code
            .clone()
            .filter(|c| c.chars().count() <= 12 && !c.contains(char::is_whitespace))
            .unwrap_or_else(|| suggest_code(&stem));
        let name = imported.name.clone().unwrap_or(stem);
        let mut info = format!(
            "{}: {} kitab, {} ayat dari {file}.",
            imported.format,
            imported.books(),
            count(imported.verses.len())
        );
        if imported.skipped > 0 {
            info.push_str(&format!(
                " {} ayat dilewati (di luar 66 kitab, nomor ganda, atau kosong).",
                count(imported.skipped)
            ));
        }
        self.bible.borrow_mut().pending = Some((imported, file));
        self.ui.set_bible_import_info(info.into());
        self.ui.set_bible_import_code(code.into());
        self.ui.set_bible_import_name(name.into());
        self.ui.set_bible_import_error(SharedString::new());
        self.ui.set_bible_importing(true);
        self.ui.set_left_tab(1);
        Ok(())
    }

    fn bible_import_save(&self) {
        let code = self.ui.get_bible_import_code();
        let name = self.ui.get_bible_import_name();
        let result = {
            let mut bible = self.bible.borrow_mut();
            let BibleState { db, pending, .. } = &mut *bible;
            let (Some(db), Some((imported, file))) = (db.as_mut(), pending.as_ref()) else {
                return;
            };
            db.add(&code, &name, file, &imported.verses)
        };
        match result {
            Err(e) => self.ui.set_bible_import_error(e.into()),
            Ok(added) => {
                {
                    let mut bible = self.bible.borrow_mut();
                    bible.pending = None;
                    if let Some(db) = bible.db.as_ref() {
                        let _ = db.set_last_translation(added.id);
                    }
                }
                self.ui.set_bible_importing(false);
                self.refresh_translations(Some(added.id));
                self.bible_lookup(true);
                self.status(
                    &format!(
                        "{} tersimpan: {} ayat. Ketik referensi untuk menampilkan ayat.",
                        added.code,
                        count(added.verses)
                    ),
                    true,
                );
            }
        }
    }

    fn bible_import_cancel(&self) {
        self.bible.borrow_mut().pending = None;
        self.ui.set_bible_importing(false);
        self.ui.set_bible_import_error(SharedString::new());
    }

    fn bible_remove_translation(&self) {
        let Some(translation) = self.current_translation() else {
            return;
        };
        let answer = dialog(
            Some(self.ui.window()),
            MessageLevel::Warning,
            "Hapus terjemahan",
            &format!(
                "Hapus {} ({} ayat) dari komputer ini?\n\n\
                 Ayat yang sudah ada di susunan atau pustaka tidak ikut terhapus. \
                 Terjemahan bisa diimpor lagi dari berkasnya.",
                translation.code,
                count(translation.verses)
            ),
            MessageButtons::YesNo,
        );
        if answer != MessageDialogResult::Yes {
            return;
        }
        self.bible_remove_now(translation.id, &translation.code);
    }

    fn bible_remove_now(&self, id: i64, code: &str) {
        let result = self.bible.borrow_mut().db.as_mut().map(|db| db.remove(id));
        if let Some(Err(e)) = result {
            dialog(
                Some(self.ui.window()),
                MessageLevel::Error,
                "Terjemahan tidak bisa dihapus",
                &e,
                MessageButtons::Ok,
            );
            return;
        }
        self.refresh_translations(None);
        self.bible_lookup(true);
        self.status(&format!("{code} dihapus dari komputer ini."), true);
    }

    // ----- Tayang -----

    fn go_live(self: &Rc<Self>) {
        if !self.commit_editor() {
            return;
        }
        let next = {
            let st = self.state.borrow();
            st.selected.map(|id| {
                (
                    st.library.slide(id, st.page),
                    st.library.service_position(id, st.selected_service),
                )
            })
        };
        match next {
            None => self.status("Pilih materi terlebih dahulu.", false),
            Some((None, _)) => self.status("Materi masih kosong. Isi teks terlebih dahulu.", false),
            Some((Some(slide), service)) => {
                {
                    let mut st = self.state.borrow_mut();
                    st.live = Some(slide);
                    st.live_service = service;
                    st.blackout = false;
                }
                self.show_output();
                self.refresh_live();
                self.status(
                    "Tayang. PageDown / PageUp: pindah slide. F6: gelapkan. F11: layar penuh.",
                    true,
                );
            }
        }
    }

    fn navigate(self: &Rc<Self>, forward: bool) {
        if !self.commit_editor() {
            return;
        }
        let (current, live_service) = {
            let st = self.state.borrow();
            (st.live.clone(), st.live_service)
        };
        let Some(current) = current else {
            self.go_live();
            return;
        };
        let from = Cursor {
            item: current.item,
            page: current.page,
            service: live_service,
        };
        let step = self.state.borrow().library.step(from, forward);
        let to = match step {
            Step::Moved(to) => to,
            Step::EndOfItem => {
                self.status(
                    "Sudah di batas materi. Tambahkan materi ke susunan untuk berpindah otomatis.",
                    false,
                );
                return;
            }
            Step::EndOfService => {
                self.status("Sudah di awal / akhir susunan ibadah.", false);
                return;
            }
        };
        let (editing, follow) = {
            let mut st = self.state.borrow_mut();
            st.live = st.library.slide(to.item, to.page);
            st.live_service = to.service;
            let follow = st.selected != Some(to.item) || st.selected_service != to.service;
            (st.editing, follow)
        };
        // Tampilan tengah mengikuti slide yang tayang, kecuali saat mengedit.
        if !editing {
            if follow {
                self.select(to.item, to.service);
            }
            self.state.borrow_mut().page = to.page;
            self.refresh_preview();
        }
        self.refresh_live();
    }

    fn toggle_blackout(&self) {
        {
            let mut st = self.state.borrow_mut();
            st.blackout = !st.blackout;
        }
        self.refresh_live();
    }

    fn refresh_monitors(&self) {
        let found = self.ui.window().with_winit_window(|w| {
            let primary = w.primary_monitor();
            let mut list: Vec<MonitorHandle> = w.available_monitors().collect();
            list.sort_by_key(|m| Some(m) != primary.as_ref());
            (list, primary)
        });
        let Some((list, primary)) = found else {
            return;
        };
        let labels: Vec<SharedString> = list
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let size = m.size();
                let main = if Some(m) == primary.as_ref() {
                    " · utama"
                } else {
                    ""
                };
                format!("Monitor {} · {}×{}{main}", i + 1, size.width, size.height).into()
            })
            .collect();
        let count = list.len();
        *self.monitors.borrow_mut() = list;
        let index = self
            .state
            .borrow()
            .library
            .monitor
            .min(count.saturating_sub(1));
        self.ui.set_monitors(ModelRc::new(VecModel::from(labels)));
        self.ui.set_monitor_index(index as i32);
    }

    fn show_output(self: &Rc<Self>) {
        if self.state.borrow().output_visible {
            return;
        }
        self.refresh_monitors();
        let several = self.monitors.borrow().len() > 1;
        {
            let mut st = self.state.borrow_mut();
            st.output_visible = true;
            st.fullscreen = several;
        }
        if self.output.show().is_err() {
            self.state.borrow_mut().output_visible = false;
            self.status("Jendela proyektor tidak bisa dibuka.", false);
            return;
        }
        self.place_output(0);
        self.refresh_live();
    }

    fn hide_output(&self) {
        let _ = self.output.hide();
        self.output_closed();
    }

    fn output_closed(&self) {
        {
            let mut st = self.state.borrow_mut();
            st.output_visible = false;
            st.fullscreen = false;
        }
        self.refresh_live();
    }

    fn toggle_projector(self: &Rc<Self>) {
        if self.state.borrow().output_visible {
            self.hide_output();
        } else {
            self.show_output();
        }
    }

    fn toggle_fullscreen(self: &Rc<Self>) {
        if !self.state.borrow().output_visible {
            self.show_output();
            self.state.borrow_mut().fullscreen = true;
        } else {
            let mut st = self.state.borrow_mut();
            st.fullscreen = !st.fullscreen;
        }
        self.place_output(0);
        self.refresh_live();
    }

    fn escape(self: &Rc<Self>) {
        if self.ui.get_picking_profile() {
            if self.ui.get_profile_form() != 0 {
                self.ui.set_profile_form(0);
            } else {
                self.close_profiles();
            }
            return;
        }
        let (visible, fullscreen) = {
            let st = self.state.borrow();
            (st.output_visible, st.fullscreen)
        };
        if fullscreen {
            self.state.borrow_mut().fullscreen = false;
            self.place_output(0);
            self.refresh_live();
        } else if visible {
            self.hide_output();
        }
    }

    /// Menaruh jendela proyektor di monitor pilihan. Jendela winit baru ada
    /// setelah `show()` diproses event loop, jadi dicoba ulang sebentar.
    fn place_output(self: &Rc<Self>, attempt: u32) {
        let (index, fullscreen) = {
            let st = self.state.borrow();
            (st.library.monitor, st.fullscreen)
        };
        let monitor = {
            let list = self.monitors.borrow();
            list.get(index).or(list.first()).cloned()
        };
        let placed = self.output.window().with_winit_window(|w| {
            w.set_fullscreen(None);
            let Some(m) = &monitor else {
                return;
            };
            let pos = m.position();
            if fullscreen {
                w.set_outer_position(pos);
                w.set_fullscreen(Some(Fullscreen::Borderless(Some(m.clone()))));
            } else {
                let scale = m.scale_factor();
                let width =
                    (f64::from(m.size().width) - 80.0 * scale).clamp(320.0 * scale, 960.0 * scale);
                let inset = (40.0 * scale) as i32;
                w.set_outer_position(PhysicalPosition::new(pos.x + inset, pos.y + inset));
                let _ = w.request_inner_size(PhysicalSize::new(
                    width as u32,
                    (width * 9.0 / 16.0) as u32,
                ));
            }
        });
        if placed.is_some() {
            window_icons(self.output.window());
            // Operator tetap bisa memakai keyboard di jendela utama.
            self.ui.window().with_winit_window(|w| w.focus_window());
        } else if attempt < 40 {
            let weak = Rc::downgrade(self);
            Timer::single_shot(Duration::from_millis(50), move || {
                if let Some(app) = weak.upgrade() {
                    app.place_output(attempt + 1);
                }
            });
        }
    }

    fn monitor_changed(self: &Rc<Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        self.state.borrow_mut().library.monitor = index;
        if self.state.borrow().output_visible {
            self.place_output(0);
        }
        self.persist();
    }

    fn background_changed(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        self.state.borrow_mut().library.background = index;
        self.ui.set_bg_index(index as i32);
        self.output.set_bg_index(index as i32);
        self.persist();
    }

    /// Memilih materi pertama di susunan, atau di pustaka bila susunan kosong.
    fn select_first(&self) {
        let first = {
            let st = self.state.borrow();
            st.library
                .service
                .first()
                .map(|id| (*id, Some(0)))
                .or_else(|| st.library.items.first().map(|i| (i.id, None)))
        };
        match first {
            Some((id, service)) => self.select(id, service),
            None => {
                self.load_editor();
                self.refresh_current();
            }
        }
    }

    // ----- Profil -----

    /// Nama dan lingkaran profil di bilah atas.
    fn refresh_profile_label(&self) {
        let (id, name) = {
            let st = self.state.borrow();
            (st.store.id().to_owned(), st.store.name().to_owned())
        };
        self.ui.set_profile_initial(profile_initial(&name).into());
        self.ui.set_profile_tone(profile_tone(&id));
        self.ui.set_profile_name(name.into());
    }

    /// Kartu di layar pilih profil. Mengembalikan jumlah profil.
    fn refresh_profiles(&self) -> usize {
        let list = match profiles(&self.data_dir) {
            Ok(list) => list,
            Err(e) => {
                self.ui.set_profile_form_error(e.into());
                return 0;
            }
        };
        let st = self.state.borrow();
        let cards: Vec<ProfileCard> = list
            .iter()
            .map(|p| {
                let current = p.id == st.store.id();
                // Profil yang terbuka dihitung dari memori, yang mungkin lebih baru.
                let count = if current {
                    st.library.items.len()
                } else {
                    p.items
                };
                ProfileCard {
                    id: p.id.as_str().into(),
                    name: p.name.as_str().into(),
                    initial: profile_initial(&p.name).into(),
                    detail: if current {
                        format!("Sedang dibuka · {count} materi")
                    } else {
                        format!("{count} materi")
                    }
                    .into(),
                    tone: profile_tone(&p.id),
                    current,
                }
            })
            .collect();
        drop(st);
        let count = cards.len();
        self.profile_cards.set_vec(cards);
        count
    }

    fn show_profiles(&self) {
        self.refresh_profiles();
        self.ui.set_profile_form(0);
        self.ui.set_profile_form_error(SharedString::new());
        self.ui.set_picking_profile(true);
    }

    fn close_profiles(&self) {
        self.ui.set_profile_form(0);
        self.ui.set_picking_profile(false);
    }

    /// `true` bila boleh berganti profil: tidak ada yang tayang, atau operator
    /// setuju tayangan dihentikan. Uji alur otomatis tidak ditanya.
    fn may_stop_live(&self) -> bool {
        if self.state.borrow().live.is_none() || self.smoke.borrow().is_some() {
            return true;
        }
        dialog(
            Some(self.ui.window()),
            MessageLevel::Warning,
            "Ganti profil",
            "Slide yang sedang tayang akan dihentikan dan layar proyektor menjadi kosong.\n\n\
             Lanjutkan?",
            MessageButtons::YesNo,
        ) == MessageDialogResult::Yes
    }

    fn choose_profile(&self, id: &str) {
        if id == self.state.borrow().store.id() {
            self.close_profiles();
            return;
        }
        if !self.may_stop_live() || !self.commit_editor() {
            return;
        }
        match Store::open_profile(&self.data_dir, id) {
            Ok(opened) => self.use_profile(opened),
            Err(e) => {
                dialog(
                    Some(self.ui.window()),
                    MessageLevel::Error,
                    "Profil tidak bisa dibuka",
                    &e,
                    MessageButtons::Ok,
                );
            }
        }
    }

    /// Formulir profil baru (1) atau ganti nama (2).
    fn submit_profile_form(&self) {
        let name = self.ui.get_profile_form_name();
        let result = match self.ui.get_profile_form() {
            1 => {
                if !self.may_stop_live() || !self.commit_editor() {
                    return;
                }
                let created = {
                    let st = self.state.borrow();
                    Store::create_profile(&self.data_dir, &name, &st.library)
                };
                created.map(|opened| self.use_profile(opened))
            }
            2 => {
                let target = self.ui.get_profile_form_target();
                let renamed = {
                    let mut st = self.state.borrow_mut();
                    if target.as_str() == st.store.id() {
                        st.store.rename(&name)
                    } else {
                        rename_profile(&self.data_dir, &target, &name)
                    }
                };
                renamed.map(|()| {
                    self.refresh_profile_label();
                    self.refresh_profiles();
                    self.ui.set_profile_form(0);
                })
            }
            _ => Ok(()),
        };
        match result {
            Ok(()) => self.ui.set_profile_form_error(SharedString::new()),
            Err(e) => self.ui.set_profile_form_error(e.into()),
        }
    }

    fn delete_profile(&self, id: &str) {
        if id == self.state.borrow().store.id() {
            self.ui.set_profile_form_error(
                "Profil yang sedang dibuka tidak bisa dihapus. Buka profil lain dulu.".into(),
            );
            return;
        }
        let name = (0..self.profile_cards.row_count())
            .filter_map(|row| self.profile_cards.row_data(row))
            .find(|card| card.id.as_str() == id)
            .map(|card| card.name.to_string())
            .unwrap_or_default();
        let answer = dialog(
            Some(self.ui.window()),
            MessageLevel::Warning,
            "Hapus profil",
            &format!(
                "Hapus profil \"{name}\"?\n\n\
                 Lirik, ayat, dan susunan ibadah di profil ini tidak akan tampil lagi di \
                 Lentera. Berkasnya dipindahkan ke folder {DELETED_PROFILES_DIR} di folder \
                 data, jadi masih bisa dipulihkan secara manual."
            ),
            MessageButtons::YesNo,
        );
        if answer == MessageDialogResult::Yes {
            self.remove_profile_now(id);
        }
    }

    fn remove_profile_now(&self, id: &str) {
        match remove_profile(&self.data_dir, id) {
            Ok(()) => {
                self.refresh_profiles();
                self.status("Profil dihapus.", true);
            }
            Err(e) => {
                dialog(
                    Some(self.ui.window()),
                    MessageLevel::Error,
                    "Profil tidak bisa dihapus",
                    &e,
                    MessageButtons::Ok,
                );
            }
        }
    }

    /// Mengganti profil yang terbuka. Pilihan monitor ikut perangkat, bukan
    /// profil, supaya layar proyektor tidak berpindah saat berganti profil.
    fn use_profile(&self, opened: Opened) {
        self.save_timer.stop();
        let background = {
            let mut st = self.state.borrow_mut();
            let monitor = st.library.monitor;
            st.store = opened.store;
            st.library = opened.library;
            st.library.monitor = monitor;
            st.selected = None;
            st.selected_service = None;
            st.page = 0;
            st.live = None;
            st.live_service = None;
            st.blackout = false;
            st.dirty = false;
            st.editing = false;
            st.filtered.clear();
            st.library.background as i32
        };
        self.ui.set_editing(false);
        self.ui.set_query(SharedString::new());
        self.ui.set_bg_index(background);
        self.output.set_bg_index(background);
        self.persist();
        self.refresh_library();
        self.refresh_service();
        self.refresh_live();
        self.select_first();
        self.refresh_profile_label();
        self.close_profiles();
        let name = self.state.borrow().store.name().to_owned();
        self.status(&format!("Profil {name} dibuka."), true);
    }

    // ----- Pembaruan dan penutupan -----

    fn check_updates(&self, source: Option<String>) {
        let ui = self.ui.as_weak();
        update::check_in_background(source, move |version| {
            let _ = ui.upgrade_in_event_loop(move |ui| {
                ui.set_status(
                    format!(
                        "Lentera {version} sudah diunduh dan akan dipasang saat Lentera ditutup."
                    )
                    .into(),
                );
                ui.set_status_ok(false);
                ui.set_update_version(version.into());
            });
        });
    }

    fn update_now(&self) {
        let version = self.ui.get_update_version();
        let answer = dialog(
            Some(self.ui.window()),
            MessageLevel::Info,
            "Perbarui Lentera",
            &format!(
                "Lentera akan ditutup, diperbarui ke {version}, lalu dibuka kembali.\n\
                 Layar proyektor ikut tertutup sebentar. Data tersimpan otomatis.\n\n\
                 Perbarui sekarang?"
            ),
            MessageButtons::YesNo,
        );
        if answer == MessageDialogResult::Yes {
            update::request_restart();
            self.close();
        }
    }

    fn close(&self) {
        self.commit_editor();
        let _ = self.output.hide();
        let _ = self.ui.hide();
        let _ = slint::quit_event_loop();
    }

    // ----- Uji alur otomatis (--ui-smoke-test) -----

    fn start_smoke(self: &Rc<Self>, report: PathBuf) {
        *self.smoke.borrow_mut() = Some(Smoke {
            step: 0,
            waits: 0,
            checks: Vec::new(),
            report,
        });
        let weak = Rc::downgrade(self);
        self.smoke_timer
            .start(TimerMode::Repeated, Duration::from_millis(450), move || {
                if let Some(app) = weak.upgrade() {
                    app.smoke_tick();
                }
            });
    }

    fn check(&self, name: &str, ok: bool) {
        if let Some(smoke) = self.smoke.borrow_mut().as_mut() {
            smoke.waits = 0;
            smoke.checks.push((name.into(), ok));
        }
    }

    /// Mengulang langkah uji ini pada detak berikutnya. Dipakai untuk keadaan
    /// jendela yang berubah tidak seketika, misalnya animasi layar penuh macOS.
    /// `false` bila batas tunggu sudah habis.
    fn retry(&self) -> bool {
        match self.smoke.borrow_mut().as_mut() {
            Some(smoke) if smoke.waits < SMOKE_MAX_WAITS => {
                smoke.waits += 1;
                smoke.step -= 1;
                true
            }
            _ => false,
        }
    }

    fn live_at(&self, item: u64, page: usize) -> bool {
        self.state
            .borrow()
            .live
            .as_ref()
            .is_some_and(|s| s.item == item && s.page == page)
    }

    fn output_fullscreen(&self) -> Option<bool> {
        self.output
            .window()
            .with_winit_window(|w| w.fullscreen().is_some())
    }

    fn smoke_tick(self: &Rc<Self>) {
        let step = match self.smoke.borrow_mut().as_mut() {
            Some(smoke) => {
                smoke.step += 1;
                smoke.step - 1
            }
            None => return,
        };
        match step {
            0 => {
                self.select_service(0);
                self.go_live();
                self.check("first_slide", self.live_at(1, 0));
            }
            1 => {
                self.navigate(true);
                self.check("next_slide", self.live_at(1, 1));
            }
            2 => {
                self.navigate(true);
                self.check("next_service_item", self.live_at(2, 0));
            }
            3 => {
                self.navigate(false);
                self.check("previous_service_item", self.live_at(1, 1));
                self.snapshot("ui-smoke-tayang.png", self.ui.window());
            }
            4 => {
                self.toggle_blackout();
                self.check(
                    "blackout",
                    self.state.borrow().blackout && self.output.get_dark(),
                );
            }
            5 => {
                self.toggle_blackout();
                if !self.state.borrow().fullscreen {
                    self.toggle_fullscreen();
                }
            }
            6 => {
                let ok = self.state.borrow().fullscreen && self.output_fullscreen() == Some(true);
                if !ok && self.retry() {
                    return;
                }
                self.check("fullscreen", ok);
                self.toggle_fullscreen();
            }
            7 => {
                let ok = !self.state.borrow().fullscreen && self.output_fullscreen() == Some(false);
                if !ok && self.retry() {
                    return;
                }
                self.check("windowed", ok);
                self.ui.set_query("BERSYUKUR".into());
                self.refresh_library();
                self.check("search", self.state.borrow().filtered == vec![1]);
                self.ui.set_query(SharedString::new());
                self.refresh_library();
                self.select_library(1);
                self.edit_current();
                self.ui.set_editor_title("Judul uji ✓".into());
                self.editor_edited();
                let saved = self.commit_editor();
                let title = self
                    .state
                    .borrow()
                    .store
                    .load()
                    .ok()
                    .and_then(|l| l.item(1).map(|i| i.title.clone()));
                self.check(
                    "unicode_edit_saved",
                    saved && title.as_deref() == Some("Judul uji ✓"),
                );
            }
            8 => {
                self.ui.set_editor_body("Satu\n\nDua\n\nTiga".into());
                self.editor_edited();
                self.check("editor_slides", self.editor_tiles.row_count() == 3);
                self.snapshot("ui-smoke-edit.png", self.ui.window());
            }
            9 => {
                self.close_editor();
                self.check(
                    "editor_closed",
                    !self.ui.get_editing() && self.slide_tiles.row_count() == 3,
                );
                self.snapshot("ui-smoke-output.png", self.output.window());
            }
            10 => {
                // Alkitab: tab kosong, lalu impor berkas TSV buatan (bukan
                // teks Alkitab).
                self.ui.set_left_tab(1);
                self.left_tab_changed(1);
                self.check(
                    "bible_empty",
                    self.bible.borrow().db.is_some()
                        && self.ui.get_bible_translations().row_count() == 0,
                );
                self.snapshot("ui-smoke-alkitab-kosong.png", self.ui.window());
                let path = self.data_dir.join("alkitab-uji.tsv");
                let mut tsv = String::from("kitab\tpasal\tayat\tteks\n");
                for verse in 1..=36 {
                    tsv.push_str(&format!(
                        "43\t3\t{verse}\tTeks uji Yohanes 3:{verse}. {}\n",
                        "Kalimat pengisi untuk menguji panjang ayat. ".repeat(verse % 4 + 1)
                    ));
                }
                let read = std::fs::write(&path, tsv)
                    .map_err(|e| e.to_string())
                    .and_then(|()| {
                        self.open_bible();
                        self.bible_read_file(&path)
                    });
                self.check(
                    "bible_file_read",
                    read.is_ok()
                        && self.ui.get_bible_importing()
                        && self.ui.get_bible_import_code() == "AU",
                );
                self.snapshot("ui-smoke-alkitab-impor.png", self.ui.window());
                self.ui.set_bible_import_code("UJI".into());
                self.ui.set_bible_import_name("Terjemahan Uji".into());
                self.bible_import_save();
                self.check(
                    "bible_imported",
                    !self.ui.get_bible_importing()
                        && self.bible.borrow().translations.len() == 1
                        && self.ui.get_left_tab() == 1,
                );
            }
            11 => {
                self.ui.set_bible_ref("yoh 3:16-18".into());
                self.bible_lookup(true);
                let selected = self.state.borrow().selected;
                self.check(
                    "bible_lookup",
                    self.ui.get_bible_has_passage()
                        && self.ui.get_bible_label() == "Yohanes 3:16-18"
                        && selected.is_some_and(Library::is_scratch)
                        && self.slide_tiles.row_count() > 0
                        && self.verse_rows.row_count() == 36,
                );
                self.snapshot("ui-smoke-alkitab.png", self.ui.window());
            }
            12 => {
                // Shift+klik ayat 20: rentang dari ayat 16.
                self.bible_verse_clicked(19, true);
                self.check(
                    "bible_shift_click",
                    self.ui.get_bible_ref() == "Yoh 3:16-20"
                        && (0..self.verse_rows.row_count())
                            .filter_map(|row| self.verse_rows.row_data(row))
                            .filter(|row| row.picked)
                            .count()
                            == 5,
                );
                self.bible_go_live();
                let live = self.state.borrow().live.as_ref().map(|s| s.item);
                self.check("bible_live", live.is_some_and(Library::is_scratch));
                let before = self.state.borrow().library.service.len();
                self.bible_add_to_service();
                let saved = self.state.borrow().store.load().ok().is_some_and(|l| {
                    l.service.len() == before + 1
                        && l.items.iter().any(|i| i.title == "Yohanes 3:16-20 (UJI)")
                });
                let st = self.state.borrow();
                let live_saved = st.live.as_ref().is_some_and(|s| {
                    !Library::is_scratch(s.item) && st.library.item(s.item).is_some()
                });
                drop(st);
                self.check("bible_saved_to_service", saved && live_saved);
                self.snapshot("ui-smoke-alkitab-tayang.png", self.output.window());
            }
            13 => {
                self.show_profiles();
                let first = self.profile_cards.row_data(0);
                self.check(
                    "profiles_listed",
                    self.profile_cards.row_count() == 1
                        && first.is_some_and(|c| c.current && c.name == DEFAULT_PROFILE_NAME),
                );
                // Membuat profil saat ada yang tayang: tayangan berhenti,
                // monitor proyektor tetap.
                let monitor = self.state.borrow().library.monitor;
                self.ui.set_profile_form(1);
                self.ui.set_profile_form_name("  Komisi Pemuda ✓ ".into());
                self.submit_profile_form();
                let st = self.state.borrow();
                let ok = st.store.name() == "Komisi Pemuda ✓"
                    && st.library.items.is_empty()
                    && st.live.is_none()
                    && st.library.monitor == monitor;
                drop(st);
                self.check(
                    "profile_created",
                    ok && !self.ui.get_picking_profile()
                        && self.library_rows.row_count() == 0
                        && self.ui.get_profile_name() == "Komisi Pemuda ✓",
                );
            }
            14 => {
                self.show_profiles();
                self.check("profile_cards", self.profile_cards.row_count() == 2);
                self.snapshot("ui-smoke-profil.png", self.ui.window());
            }
            15 => {
                let id_of = |name: &str| {
                    profiles(&self.data_dir)
                        .ok()
                        .and_then(|list| list.into_iter().find(|p| p.name == name))
                        .map(|p| p.id)
                        .unwrap_or_default()
                };
                let first = id_of(DEFAULT_PROFILE_NAME);
                self.ui.set_profile_form(2);
                self.ui.set_profile_form_target(first.as_str().into());
                self.ui.set_profile_form_name("Ibadah Umum".into());
                self.submit_profile_form();
                self.check("profile_renamed", id_of("Ibadah Umum") == first);
                self.choose_profile(&first);
                let title = self.state.borrow().library.item(1).map(|i| i.title.clone());
                self.check(
                    "profile_switched",
                    self.state.borrow().store.id() == first
                        && title.as_deref() == Some("Judul uji ✓")
                        && !self.ui.get_picking_profile(),
                );
            }
            16 => {
                let other = profiles(&self.data_dir)
                    .ok()
                    .and_then(|list| list.into_iter().find(|p| p.name == "Komisi Pemuda ✓"))
                    .map(|p| p.id)
                    .unwrap_or_default();
                self.remove_profile_now(&other);
                self.check(
                    "profile_removed",
                    profiles(&self.data_dir).is_ok_and(|list| list.len() == 1),
                );
                // Ukuran jendela terkecil yang didukung.
                self.ui
                    .window()
                    .set_size(slint::LogicalSize::new(1024.0, 640.0));
            }
            17 => {
                self.snapshot("ui-smoke-sempit.png", self.ui.window());
                // Impor banyak lagu (jalur yang sama dengan impor
                // EasyWorship): yang sudah ada dilewati, judul kembar diberi
                // nomor, lalu tersimpan.
                let title = "Lagu Impor Uji";
                let songs = [
                    (title, "Baris satu\n\nBaris dua"),
                    (title, "Lirik versi lain"),
                    ("LAGU IMPOR UJI", "baris satu, baris dua"),
                ]
                .map(|(t, c)| (t.to_owned(), c.to_owned()));
                let plan = self.state.borrow().library.plan_song_import(songs);
                let counted = (plan.songs.len(), plan.existing, plan.renamed) == (2, 1, 1);
                let added = self.add_imported_songs(plan);
                let saved = self
                    .state
                    .borrow()
                    .store
                    .load()
                    .ok()
                    .is_some_and(|l| l.items.iter().any(|i| i.title == "Lagu Impor Uji (2)"));
                let selected = self.state.borrow().selected;
                let first_selected = selected
                    .and_then(|id| {
                        self.state
                            .borrow()
                            .library
                            .item(id)
                            .map(|i| i.title.clone())
                    })
                    .is_some_and(|t| t == title);
                self.check(
                    "songs_imported",
                    counted && added == 2 && saved && first_selected,
                );
                self.ui.invoke_show_import_menu();
            }
            18 => {
                self.snapshot("ui-smoke-impor.png", self.ui.window());
                self.ui.invoke_close_import_menu();
                self.ui.invoke_show_about();
            }
            19 => {
                self.snapshot("ui-smoke-tentang.png", self.ui.window());
            }
            _ => {
                self.smoke_timer.stop();
                self.write_smoke_report();
                self.close();
            }
        }
    }

    fn write_smoke_report(&self) {
        let Some(smoke) = self.smoke.borrow_mut().take() else {
            return;
        };
        let (output_visible, data_saved) = {
            let st = self.state.borrow();
            (st.output_visible, st.store.path().exists())
        };
        let failed: Vec<&str> = smoke
            .checks
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(name, _)| name.as_str())
            .collect();
        let report = serde_json::json!({
            "ok": failed.is_empty(),
            "failed": failed,
            "checks": smoke.checks,
            "version": VERSION,
            "ui": "slint",
            "output_window": output_visible,
            "monitors": self.monitors.borrow().len(),
            "data_saved": data_saved,
        });
        let _ = std::fs::write(
            &smoke.report,
            serde_json::to_vec_pretty(&report).unwrap_or_default(),
        );
    }

    #[cfg(feature = "snapshot")]
    fn snapshot(&self, name: &str, window: &slint::Window) {
        let dir = self
            .smoke
            .borrow()
            .as_ref()
            .and_then(|s| s.report.parent().map(|p| p.to_path_buf()));
        let (Some(dir), Ok(buffer)) = (dir, window.take_snapshot()) else {
            return;
        };
        let Ok(file) = std::fs::File::create(dir.join(name)) else {
            return;
        };
        let mut encoder = png::Encoder::new(
            std::io::BufWriter::new(file),
            buffer.width(),
            buffer.height(),
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        if let Ok(mut writer) = encoder.write_header() {
            let _ = writer.write_image_data(buffer.as_bytes());
        }
    }

    #[cfg(not(feature = "snapshot"))]
    fn snapshot(&self, _name: &str, _window: &slint::Window) {}
}

/// Nomor grup ikon di resource lentera.exe (ditulis oleh build.rs).
#[cfg(windows)]
const APP_ICON: u16 = 1;

/// Memasang ikon title bar (16 px) dan taskbar (32 px) dari resource
/// lentera.exe sesuai skala layar jendela, supaya Windows memakai gambar kecil
/// yang memang digambar untuk ukuran itu, bukan ikon besar yang diperkecil.
#[cfg(windows)]
fn window_icons(window: &slint::Window) {
    use winit::platform::windows::{IconExtWindows, WindowExtWindows};
    use winit::window::Icon;
    window.with_winit_window(|w| {
        let size = |logical: f64| {
            let px = (logical * w.scale_factor()).round() as u32;
            Some(PhysicalSize::new(px, px))
        };
        if let Ok(icon) = Icon::from_resource(APP_ICON, size(16.0)) {
            w.set_window_icon(Some(icon));
        }
        if let Ok(icon) = Icon::from_resource(APP_ICON, size(32.0)) {
            w.set_taskbar_icon(Some(icon));
        }
    });
}

/// Di macOS ikon aplikasi berasal dari paket .app, bukan dari jendela.
#[cfg(not(windows))]
fn window_icons(_window: &slint::Window) {}

fn run(args: &Args) -> Result<(), String> {
    if args.smoke && args.data_dir.is_none() {
        return Err("Pengujian UI wajib memakai --data-dir terpisah.".into());
    }
    let dir = args.data_dir.clone().unwrap_or_else(default_data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Folder data tidak bisa dibuat: {e}"))?;
    let lock = DataLock::acquire(&dir.join(LOCK_FILE))?;
    // Profil pertama; dibuat (dan `library.json` lama dipindahkan) bila belum ada.
    let Opened {
        store,
        library,
        fresh: is_new,
    } = Store::open(&dir)?;

    let backend = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into());
    // Ikon jendela sejak dibuat (operator dan proyektor). Ukuran pastinya
    // dipasang lagi oleh `window_icons` setelah skala layar diketahui.
    #[cfg(windows)]
    let backend = backend.with_winit_window_attributes_hook(|attributes| {
        use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
        let icon = || winit::window::Icon::from_resource(APP_ICON, None).ok();
        attributes
            .with_window_icon(icon())
            .with_taskbar_icon(icon())
    });
    backend
        .select()
        .map_err(|e| format!("Tampilan tidak bisa dimulai: {e}"))?;

    let app = App::new(library, store, lock, dir.clone())?;
    app.connect();
    app.refresh_library();
    app.refresh_service();
    app.refresh_live();
    app.select_first();
    app.refresh_profile_label();
    app.persist();
    // Lebih dari satu profil: tanyakan dulu siapa yang memakai. Profil yang
    // terakhir dibuka sudah terbuka di belakangnya.
    if !args.smoke && app.refresh_profiles() > 1 {
        app.show_profiles();
    }
    app.ui.show().map_err(|e| e.to_string())?;

    let weak = Rc::downgrade(&app);
    let smoke = args.smoke.then(|| dir.join("ui-smoke-result.json"));
    let source = args.update_source.clone();
    Timer::single_shot(Duration::from_millis(300), move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        window_icons(app.ui.window());
        app.refresh_monitors();
        if is_new && app.monitors.borrow().len() > 1 {
            app.state.borrow_mut().library.monitor = 1;
            app.ui.set_monitor_index(1);
            app.persist();
        }
        match smoke {
            Some(report) => app.start_smoke(report),
            None => app.check_updates(source),
        }
    });

    slint::run_event_loop().map_err(|e| e.to_string())?;
    // Melepas kunci folder data sebelum pembaruan dipasang.
    drop(app);
    Ok(())
}

fn main() {
    update::startup();
    let args = parse_args();
    if let Err(error) = run(&args) {
        dialog(
            None,
            MessageLevel::Error,
            "Lentera tidak bisa dibuka",
            &error,
            MessageButtons::Ok,
        );
        std::process::exit(1);
    }
    update::apply_after_exit(&args.raw);
}
