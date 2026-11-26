//! Lentera dengan tampilan Slint untuk Windows dan macOS.
//!
//! Data, susunan ibadah, dan penyimpanan ada di `lentera-core`. Berkas ini
//! menghubungkan data itu ke tampilan di `ui/app.slint`: jendela operator dan
//! jendela proyektor.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod akun;
#[cfg(windows)]
mod dialog_guard;
mod update;
mod video;

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    ffi::OsString,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use lentera_core::background::{self, Added, Picture};
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
    CloseRequestResponse, ComponentHandle, Image, Model, ModelRc, Rgb8Pixel, Rgba8Pixel,
    SharedPixelBuffer, SharedString, Timer, TimerMode, VecModel,
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
    /// Alamat server akun, mengganti alamat bawaan rilis.
    server: Option<String>,
    smoke: bool,
    /// Argumen asli, dipakai lagi saat Lentera dibuka ulang setelah pembaruan.
    raw: Vec<OsString>,
}

fn parse_args() -> Args {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut args = Args {
        data_dir: None,
        update_source: None,
        server: None,
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
            Some("--server") => {
                args.server = items.next().map(|v| v.to_string_lossy().into_owned())
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
    format!("{} · {} slide", kind_label(item.kind), item.slide_count())
}

/// Keterangan di samping label jenis, di atas judul materi terpilih. Urutan
/// di susunan ditambahkan oleh tampilan.
fn current_meta(item: &Item) -> String {
    if Library::is_scratch(item.id) {
        format!("{} slide · belum di pustaka", item.slide_count())
    } else {
        format!("{} slide", item.slide_count())
    }
}

/// Nama warna latar umum, urutannya sama dengan `Theme.slide-names`.
const SLIDE_NAMES: [&str; 3] = ["Gelap", "Biru malam", "Terang"];

/// Slide tanpa keterangan referensi; keterangan hanya untuk slide yang
/// tayang (lihat `refresh_live`).
fn tile(text: &str, verse: bool) -> SlideTile {
    SlideTile {
        text: text.into(),
        first: text.lines().next().unwrap_or_default().into(),
        lines: text.lines().count().max(1) as i32,
        chars: text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as i32,
        caption: SharedString::new(),
        verse,
    }
}

fn tiles(slides: &[String], verse: bool) -> Vec<SlideTile> {
    slides.iter().map(|s| tile(s, verse)).collect()
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

/// Tulisan baku referensi Alkitab bila teks pencarian berupa referensi
/// lengkap dengan pasal: "yoh 3:16" → "Yohanes 3:16". Nama kitab saja
/// tidak dihitung, supaya pencarian lagu biasa tidak terganggu.
fn reference_label(text: &str) -> Option<String> {
    bible::parse_reference(text)
        .ok()
        .filter(|r| !r.spans.is_empty())
        .map(|r| r.label())
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

/// Gambar dari inti Lentera untuk Slint.
fn to_image(picture: Picture) -> Image {
    let buffer = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
        &picture.rgb,
        picture.width,
        picture.height,
    );
    Image::from_rgb8(buffer)
}

/// Nama latar tanpa ekstensi, untuk ditampilkan.
fn background_label(name: &str) -> &str {
    background::label(name)
}

/// Membuat lagi gambar diam video latar `name` bila hilang, misalnya setelah
/// folder `posters` terhapus. Butuh sepersekian detik.
fn ensure_poster(profile_dir: &Path, name: &str) {
    if !background::is_video(name) || background::has_poster(profile_dir, name) {
        return;
    }
    if let Ok(path) = background::video_path(profile_dir, name)
        && let Ok(poster) = video::poster(&path)
    {
        let _ = background::save_poster(profile_dir, name, &poster);
    }
}

/// Video untuk uji alur (`--ui-smoke-test`), dari variabel LENTERA_SMOKE_VIDEO.
fn smoke_video() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("LENTERA_SMOKE_VIDEO")?);
    (video::SUPPORTED && path.is_file()).then_some(path)
}

/// Menambahkan satu gambar atau video ke latar profil. Dijalankan di thread
/// lain: memperkecil foto atau membuka video butuh waktu di PC lemah.
fn add_background_file(profile_dir: &Path, path: &Path) -> Result<Added, String> {
    if background::is_video_file(path) {
        let poster = video::poster(path)?;
        background::add_video(profile_dir, path, &poster)
    } else {
        background::add(profile_dir, path)
    }
}

/// "Monitor 2" di tombol status proyektor; kosong bila monitor tidak terbaca.
fn projector_label(index: usize, count: usize) -> String {
    if index < count {
        format!("Monitor {}", index + 1)
    } else {
        String::new()
    }
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
    // Kotak pesan yang dibuka saat jendela masih perlu digambar ulang (misalnya
    // tepat setelah menu sembulan ditutup) tidak tampil sendiri.
    #[cfg(windows)]
    let _guard = dialog_guard::DialogGuard::start(title);
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
    /// Ayat yang tampil di tengah: judul materi, isi, dan keterangan
    /// (terjemahan, jumlah ayat dan slide) tanpa letaknya di pustaka.
    shown: Option<(String, String, String)>,
}

/// Gambar latar yang sedang ditambahkan di thread lain.
struct Adding {
    results: mpsc::Receiver<Result<Added, String>>,
    /// Profil dan sasaran galeri saat penambahan dimulai (lihat
    /// `App::bg_target`). Satu gambar yang ditambahkan langsung dipakai di sana.
    profile: String,
    target: Option<u64>,
    total: usize,
    added: Vec<String>,
    existing: Vec<String>,
    failed: Vec<String>,
}

impl Adding {
    fn done(&self) -> usize {
        self.added.len() + self.existing.len() + self.failed.len()
    }
}

struct Smoke {
    step: u32,
    /// Berapa kali langkah ini sudah menunggu (lihat `App::retry`).
    waits: u32,
    checks: Vec<(String, bool)>,
    report: PathBuf,
    /// Akhir jeda pemutaran video (LENTERA_SMOKE_HOLD).
    hold_until: Option<Instant>,
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
    background_tiles: Rc<VecModel<BackgroundTile>>,
    /// Gambar latar yang sedang dipakai (latar umum, materi terpilih, materi
    /// tayang), menurut namanya. Gambar lain dilepas dari memori.
    pictures: RefCell<HashMap<String, Image>>,
    /// Sasaran galeri latar: materi ini, atau latar umum bila `None`.
    bg_target: Cell<Option<u64>>,
    /// Gambar kecil daftar latar profil yang sedang dibuka.
    bg_thumbs: RefCell<HashMap<String, Image>>,
    /// Video latar yang sedang diputar di proyektor, menurut letak berkasnya.
    video: RefCell<Option<(PathBuf, video::Player)>>,
    /// Naik setiap kali video berganti atau berhenti; frame dari pemutar lama
    /// yang masih di jalan diabaikan.
    video_serial: Arc<AtomicU64>,
    /// Jumlah frame video yang sudah tampil di proyektor (untuk uji alur).
    video_frames: Arc<AtomicU64>,
    adding: RefCell<Option<Adding>>,
    adding_timer: Timer,
    bible: RefCell<BibleState>,
    /// Folder data (berisi semua profil).
    data_dir: PathBuf,
    monitors: RefCell<Vec<MonitorHandle>>,
    save_timer: Timer,
    status_timer: Timer,
    smoke_timer: Timer,
    smoke: RefCell<Option<Smoke>>,
    /// Server akun, bila Lentera memakai akun.
    server: RefCell<Option<String>>,
    /// Akun yang masuk dan putaran sinkronnya.
    account: RefCell<Option<akun::Account>>,
    sync_timer: Timer,
    sync_busy_timer: Timer,
    /// Layar masuk lagi yang sedang terbuka.
    relogin: RefCell<Option<Rc<akun::Login>>>,
    /// Referensi ke diri sendiri untuk callback timer dan thread latar.
    me: Weak<App>,
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
        let app = Rc::new_cyclic(|me| Self {
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
            background_tiles: Rc::new(VecModel::default()),
            pictures: RefCell::default(),
            bg_target: Cell::default(),
            bg_thumbs: RefCell::default(),
            video: RefCell::default(),
            video_serial: Arc::default(),
            video_frames: Arc::default(),
            adding: RefCell::default(),
            adding_timer: Timer::default(),
            bible: RefCell::default(),
            data_dir,
            monitors: RefCell::default(),
            save_timer: Timer::default(),
            status_timer: Timer::default(),
            smoke_timer: Timer::default(),
            smoke: RefCell::default(),
            server: RefCell::default(),
            account: RefCell::default(),
            sync_timer: Timer::default(),
            sync_busy_timer: Timer::default(),
            relogin: RefCell::default(),
            me: me.clone(),
            _lock: lock,
        });
        let ui = &app.ui;
        ui.set_version(VERSION.into());
        ui.set_video_supported(video::SUPPORTED);
        ui.set_library(ModelRc::from(app.library_rows.clone()));
        ui.set_service(ModelRc::from(app.service_rows.clone()));
        ui.set_slides(ModelRc::from(app.slide_tiles.clone()));
        ui.set_editor_slides(ModelRc::from(app.editor_tiles.clone()));
        ui.set_profiles(ModelRc::from(app.profile_cards.clone()));
        ui.set_bible_verses(ModelRc::from(app.verse_rows.clone()));
        ui.set_backgrounds(ModelRc::from(app.background_tiles.clone()));
        ui.set_bg_index(background);
        app.output.set_bg_index(background);
        app.apply_background();
        Ok(app)
    }

    fn connect(self: &Rc<Self>) {
        let app = self.clone();
        connect!(app.ui.on_search_changed, |a| a.search_changed());
        connect!(app.ui.on_search_accepted, |a| a.search_accepted());
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
        connect!(app.ui.on_projector_menu_opened, |a| a.refresh_monitors());
        connect!(app.ui.on_escape, |a| a.escape());
        connect!(app.ui.on_monitor_changed, |a, index| a
            .monitor_changed(index));
        connect!(app.ui.on_background_changed, |a, index| a
            .background_changed(index));
        connect!(app.ui.on_show_backgrounds, |a| a.show_backgrounds(None));
        connect!(app.ui.on_show_item_backgrounds, |a| a
            .show_item_backgrounds());
        connect!(app.ui.on_use_general_background, |a| a
            .use_general_background());
        connect!(app.ui.on_pick_background, |a, name| a
            .pick_background(&name));
        connect!(app.ui.on_add_backgrounds, |a| a.add_backgrounds());
        connect!(app.ui.on_remove_background, |a, name| a
            .remove_background(&name));
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
        connect!(app.ui.on_sync_now, |a| a.sync_now());
        connect!(app.ui.on_sign_in_again, |a| a.sign_in_again());
        connect!(app.ui.on_sign_out, |a| a.sign_out());
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
        connect!(app.ui.on_bible_history_clicked, |a, index| a
            .bible_history_clicked(index));
        connect!(app.ui.on_bible_chapter, |a, chapter| a
            .bible_chapter(chapter));

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
                in_service: st.library.service.contains(&item.id),
            })
            .collect();
        st.filtered = rows.iter().map(|r| r.id as u64).collect();
        drop(st);
        self.ui.set_library_count(count(rows.len()).into());
        self.library_rows.set_vec(rows);
        self.ui
            .set_query_reference(reference_label(&query).unwrap_or_default().into());
        self.refresh_bible_detail();
    }

    fn search_changed(&self) {
        self.refresh_library();
        // Hasil pencarian ada di tab Pustaka.
        if !self.ui.get_query().is_empty() && self.ui.get_left_tab() != 0 {
            self.ui.set_left_tab(0);
            self.left_tab_changed(0);
        }
    }

    /// Enter di kotak cari: referensi Alkitab dibuka di tab Alkitab.
    fn search_accepted(&self) {
        let query = self.ui.get_query();
        if reference_label(&query).is_none() {
            return;
        }
        self.ui.set_query(SharedString::new());
        self.refresh_library();
        self.ui.set_bible_ref(query.trim().into());
        self.ui.set_left_tab(1);
        if self.show_bible() {
            self.bible_lookup(true);
            self.ui.set_bible_focus(true);
        }
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
        self.mark_library_in_service();
        self.refresh_bible_detail();
    }

    /// Memperbarui tanda "Di susunan" di daftar pustaka tanpa menyusun ulang
    /// daftarnya, supaya posisi gulir tetap.
    fn mark_library_in_service(&self) {
        let st = self.state.borrow();
        for i in 0..self.library_rows.row_count() {
            let Some(mut row) = self.library_rows.row_data(i) else {
                continue;
            };
            let in_service = st.library.service.contains(&(row.id as u64));
            if row.in_service != in_service {
                row.in_service = in_service;
                self.library_rows.set_row_data(i, row);
            }
        }
    }

    /// Materi terpilih: judul, semua slide, dan pratinjau.
    fn refresh_current(&self) {
        let st = self.state.borrow();
        let current = st.selected.and_then(|id| st.library.item(id)).map(|item| {
            (
                item.id,
                item.title.clone(),
                current_meta(item),
                item.kind == Kind::Song,
                item.slides(),
            )
        });
        drop(st);
        self.apply_view_background();
        let Some((id, title, meta, song, slides)) = current else {
            self.ui.set_has_current(false);
            self.ui.set_selected_id(-1);
            self.slide_tiles.set_vec(Vec::new());
            self.refresh_preview();
            return;
        };
        self.ui.set_has_current(true);
        self.ui.set_selected_id(ui_id(id));
        self.ui.set_current_title(title.into());
        self.ui.set_current_meta(meta.into());
        self.ui.set_current_song(song);
        self.slide_tiles.set_vec(tiles(&slides, !song));
        self.refresh_preview();
    }

    fn refresh_preview(&self) {
        let mut st = self.state.borrow_mut();
        let current = st
            .selected
            .and_then(|id| st.library.item(id))
            .map(|item| (item.title.clone(), item.kind == Kind::Verse, item.slides()));
        let Some((title, verse, slides)) = current.filter(|(_, _, slides)| !slides.is_empty())
        else {
            drop(st);
            self.ui.set_has_preview(false);
            self.ui.set_preview(SlideTile::default());
            self.ui.set_preview_meta(SharedString::new());
            self.ui.set_preview_title(SharedString::new());
            self.ui.set_preview_page(-1);
            return;
        };
        st.page = st.page.min(slides.len() - 1);
        let page = st.page;
        drop(st);
        self.ui.set_has_preview(true);
        self.ui.set_preview(tile(&slides[page], verse));
        self.ui
            .set_preview_meta(format!("{title} · {}/{}", page + 1, slides.len()).into());
        self.ui
            .set_preview_title(format!("Slide {} · {title}", page + 1).into());
        self.ui.set_preview_page(page as i32);
    }

    fn refresh_live(&self) {
        let st = self.state.borrow();
        let live_tile = st
            .live
            .as_ref()
            .map(|s| SlideTile {
                caption: s.caption.as_str().into(),
                ..tile(&s.text, s.kind == Kind::Verse)
            })
            .unwrap_or_default();
        let meta = match (&st.live, st.blackout) {
            (_, true) => "Layar digelapkan".to_owned(),
            (Some(s), false) => format!("{} · {}/{}", s.title, s.page + 1, s.total),
            (None, false) => "Belum ada yang tayang".to_owned(),
        };
        let has_live = st.live.is_some();
        let live_title = st
            .live
            .as_ref()
            .map(|s| s.title.clone())
            .unwrap_or_default();
        let live_total = st.live.as_ref().map_or(0, |s| s.total as i32);
        let item = st.live.as_ref().map_or(-1, |s| ui_id(s.item));
        let page = st.live.as_ref().map_or(-1, |s| s.page as i32);
        let service = ui_index(st.live_service);
        let (blackout, visible, fullscreen) = (st.blackout, st.output_visible, st.fullscreen);
        drop(st);
        self.ui.set_live(live_tile.clone());
        self.ui.set_has_live(has_live);
        self.ui.set_live_meta(meta.into());
        self.ui.set_live_title(live_title.into());
        self.ui.set_live_total(live_total);
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
        self.apply_live_background();
    }

    fn load_editor(&self) {
        let st = self.state.borrow();
        let item = st.selected.and_then(|id| st.library.item(id)).cloned();
        drop(st);
        match item {
            Some(item) => {
                self.editor_tiles
                    .set_vec(tiles(&item.slides(), item.kind == Kind::Verse));
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
        self.ui.set_editor_dirty(false);
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
        if !Library::is_scratch(id) {
            self.ui.set_bible_view(false);
        }
        self.ui.set_editor_dirty(false);
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
        self.ui.set_bible_view(false);
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
        self.ui.set_editor_dirty(true);
        let body = self.ui.get_editor_body();
        self.editor_tiles
            .set_vec(tiles(&split_slides(&body), !self.ui.get_editor_song()));
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
        let body = self.ui.get_editor_body();
        self.editor_tiles
            .set_vec(tiles(&split_slides(&body), !song));
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
        self.ui.set_editor_dirty(false);
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

    /// Seperti [`App::pick_file_then`], untuk beberapa berkas sekaligus.
    fn pick_files_then(
        self: &Rc<Self>,
        picker: rfd::FileDialog,
        then: impl FnOnce(&Rc<Self>, Vec<PathBuf>) + 'static,
    ) {
        let Some(paths) = picker
            .set_parent(&self.ui.window().window_handle())
            .pick_files()
        else {
            return;
        };
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::from_millis(50), move || {
            if let Some(app) = weak.upgrade() {
                then(&app, paths);
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
        if tab != 1 {
            self.ui.set_bible_view(false);
            return;
        }
        if self.show_bible() {
            // Ayat terakhir tampil lagi di tengah.
            self.bible_lookup(false);
        }
    }

    /// Menampilkan Alkitab di kolom tengah. `false` bila editor tidak bisa
    /// ditutup atau `alkitab.db` tidak bisa dibuka.
    fn show_bible(&self) -> bool {
        if self.state.borrow().editing {
            self.close_editor();
            if self.state.borrow().editing {
                return false;
            }
        }
        if !self.open_bible() {
            return false;
        }
        self.ui.set_bible_view(true);
        self.refresh_history();
        self.bible_ready_status();
        true
    }

    /// "Terjemahan Baru siap · 31.102 ayat" di bilah bawah.
    fn bible_ready_status(&self) {
        if let Some(t) = self.current_translation() {
            self.status(&format!("{} siap · {} ayat", t.name, count(t.verses)), true);
        }
    }

    /// Daftar "Terakhir dibuka" di tab Alkitab.
    fn refresh_history(&self) {
        let list = self
            .bible
            .borrow()
            .db
            .as_ref()
            .map(Bible::history)
            .unwrap_or_default();
        let rows: Vec<HistoryRow> = list
            .into_iter()
            .map(|r| HistoryRow {
                label: r.label.into(),
                detail: format!("{} ayat", r.verses).into(),
            })
            .collect();
        self.ui
            .set_bible_history(ModelRc::new(VecModel::from(rows)));
    }

    /// Mencatat ayat yang tampil di tengah ke "Terakhir dibuka".
    fn remember_passage(&self) {
        let result = {
            let bible = self.bible.borrow();
            let (Some(db), Some(found)) = (bible.db.as_ref(), bible.lookup.as_ref()) else {
                return;
            };
            if found.verses.is_empty() {
                return;
            }
            db.remember(&found.label, found.verses.len())
        };
        match result {
            Ok(()) => self.refresh_history(),
            Err(e) => self.status(&e, false),
        }
    }

    fn bible_history_clicked(&self, index: i32) {
        let label = usize::try_from(index)
            .ok()
            .and_then(|i| self.ui.get_bible_history().row_data(i))
            .map(|row| row.label);
        let Some(label) = label else {
            return;
        };
        self.ui.set_bible_ref(label);
        if self.show_bible() {
            self.bible_lookup(true);
        }
    }

    /// Tombol pasal sebelumnya / berikutnya: ayat pertama pasal itu.
    fn bible_chapter(&self, chapter: i32) {
        let short = self
            .bible
            .borrow()
            .lookup
            .as_ref()
            .map(|found| found.reference.book.short);
        let (Some(short), Ok(chapter)) = (short, u16::try_from(chapter)) else {
            return;
        };
        if chapter == 0 {
            return;
        }
        self.ui.set_bible_ref(format!("{short} {chapter}:1").into());
        self.bible_lookup(true);
    }

    fn bible_translation_changed(&self) {
        if let Some(translation) = self.current_translation()
            && let Some(db) = self.bible.borrow().db.as_ref()
        {
            let _ = db.set_last_translation(translation.id);
        }
        // Ayatnya tampil di tengah dengan terjemahan yang baru, juga bila
        // tengah sedang menampilkan materi lain.
        if self.show_bible() {
            self.bible_lookup(true);
        }
    }

    /// Mengosongkan hasil referensi di tab Alkitab.
    fn clear_bible_result(&self) {
        {
            let mut bible = self.bible.borrow_mut();
            bible.lookup = None;
            bible.anchor = None;
            bible.shown = None;
        }
        self.verse_rows.set_vec(Vec::new());
        self.ui.set_bible_label(SharedString::new());
        self.ui.set_bible_detail(SharedString::new());
        self.ui.set_bible_error(SharedString::new());
        self.ui.set_bible_chapter_title(SharedString::new());
        self.ui.set_bible_prev_chapter(0);
        self.ui.set_bible_next_chapter(0);
        self.ui.set_bible_has_passage(false);
        self.ui.set_bible_showing(false);
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
                text: v.text.as_str().into(),
                picked: picked.contains(&(v.chapter, v.verse)),
            })
            .collect();
        let first = rows.iter().position(|r| r.picked);
        let same_chapter = self.bible.borrow().lookup.as_ref().is_some_and(|old| {
            old.reference.book.number == book.number && old.chapter == found.chapter
        });
        if same_chapter && rows.len() == self.verse_rows.row_count() {
            // Pasal yang sama: hanya tanda pilihan yang berubah, jadi daftar
            // tidak disusun ulang dan posisi gulirnya tetap.
            for (i, row) in rows.into_iter().enumerate() {
                if self
                    .verse_rows
                    .row_data(i)
                    .is_none_or(|old| old.picked != row.picked)
                {
                    self.verse_rows.set_row_data(i, row);
                }
            }
        } else {
            self.verse_rows.set_vec(rows);
        }
        self.ui.set_bible_error(SharedString::new());
        self.ui
            .set_bible_chapter_title(format!("{} {}", book.name, found.chapter).into());
        let chapter = i32::from(found.chapter);
        self.ui
            .set_bible_prev_chapter(if chapter > 1 { chapter - 1 } else { 0 });
        self.ui
            .set_bible_next_chapter(if found.chapter < found.chapters {
                chapter + 1
            } else {
                0
            });
        // Daftar yang baru tampil lagi digulir ke ayat pertama yang dipilih.
        self.ui.set_bible_scroll_row(first.unwrap_or(0) as i32);
        if typed || !same_chapter {
            let mut bible = self.bible.borrow_mut();
            bible.scroll_serial += 1;
            if typed {
                bible.anchor = first;
            }
            self.ui.set_bible_scroll_serial(bible.scroll_serial);
        }
        if found.verses.is_empty() {
            // Baru nama kitab: daftar pasal pertama untuk dipilih.
            self.ui.set_bible_label(book.name.into());
            self.ui
                .set_bible_detail("Ketik pasal dan ayat, atau klik ayat di bawah.".into());
            self.ui.set_bible_has_passage(false);
            self.ui.set_bible_showing(false);
            let mut bible = self.bible.borrow_mut();
            bible.lookup = Some(found);
            bible.shown = None;
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
        let about = format!(
            "{} · {} ayat · {slides} slide",
            translation.name,
            found.verses.len()
        );
        self.ui.set_bible_label(found.label.as_str().into());
        self.ui.set_bible_has_passage(true);
        self.ui.set_bible_showing(true);
        {
            let mut bible = self.bible.borrow_mut();
            bible.lookup = Some(found);
            bible.shown = Some((title.clone(), content.clone(), about));
        }
        self.show_scratch(title, content);
        self.refresh_bible_detail();
    }

    /// Keterangan ayat di tengah, termasuk apakah ayat itu sudah ada di
    /// pustaka atau susunan.
    fn refresh_bible_detail(&self) {
        let detail = {
            let bible = self.bible.borrow();
            let Some((title, content, about)) = bible.shown.as_ref() else {
                return;
            };
            let st = self.state.borrow();
            let saved = st
                .library
                .items
                .iter()
                .find(|i| {
                    !Library::is_scratch(i.id)
                        && i.kind == Kind::Verse
                        && &i.title == title
                        && &i.content == content
                })
                .map(|i| i.id);
            let place = match saved {
                Some(id) if st.library.service.contains(&id) => "ada di susunan",
                Some(_) => "ada di pustaka",
                None => "belum di pustaka",
            };
            format!("{about} · {place}")
        };
        self.ui.set_bible_detail(detail.into());
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
            self.remember_passage();
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
        self.show_bible();
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
        if self.show_bible() {
            self.bible_lookup(true);
        }
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
                let passage = Library::is_scratch(slide.item);
                {
                    let mut st = self.state.borrow_mut();
                    st.live = Some(slide);
                    st.live_service = service;
                    st.blackout = false;
                }
                self.show_output();
                self.refresh_live();
                if passage {
                    self.remember_passage();
                }
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
        self.ui
            .set_projector_label(projector_label(index, count).into());
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
        let count = self.monitors.borrow().len();
        self.ui
            .set_projector_label(projector_label(index, count).into());
        if self.state.borrow().output_visible {
            self.place_output(0);
        }
        self.persist();
    }

    // ----- Latar -----

    /// Warna latar umum dipilih; gambar latar umum tidak dipakai lagi.
    fn background_changed(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        {
            let mut st = self.state.borrow_mut();
            st.library.background = index;
            st.library.background_image = None;
        }
        self.apply_background();
        self.persist();
        if self.selected_has_own_background() {
            self.status(
                "Latar umum diganti. Materi ini tetap memakai latarnya sendiri.",
                true,
            );
        }
    }

    /// Materi terpilih punya gambar latar sendiri.
    fn selected_has_own_background(&self) -> bool {
        let st = self.state.borrow();
        st.selected
            .and_then(|id| st.library.item(id))
            .is_some_and(|item| item.background.is_some())
    }

    /// Folder profil yang sedang dibuka.
    fn profile_dir(&self) -> PathBuf {
        let st = self.state.borrow();
        st.store
            .path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    /// Menerapkan latar ke semua layar: latar umum (kotak Latar umum), latar
    /// materi terpilih (slide, pratinjau, editor), dan latar materi yang
    /// tayang (layar Tayang dan proyektor). Materi tanpa gambar sendiri
    /// memakai latar umum.
    fn apply_background(&self) {
        let index = self.state.borrow().library.background as i32;
        self.ui.set_bg_index(index);
        self.output.set_bg_index(index);
        let general = self.general_picture();
        self.ui.set_has_bg_picture(general.is_some());
        self.ui.set_bg_picture(general.unwrap_or_default());
        self.apply_view_background();
        self.apply_live_background();
    }

    /// Latar materi terpilih: slide di tengah, pratinjau, dan hasil editor.
    fn apply_view_background(&self) {
        let selected = self.state.borrow().selected;
        let picture = self.picture_for(selected);
        self.ui.set_has_view_picture(picture.is_some());
        self.ui.set_view_picture(picture.unwrap_or_default());
        let own = {
            let st = self.state.borrow();
            selected
                .and_then(|id| st.library.item(id))
                .and_then(|item| item.background.as_deref().map(background_label))
                .unwrap_or_default()
                .to_owned()
        };
        self.ui.set_current_background(own.into());
        self.prune_pictures();
    }

    /// Latar materi yang sedang tayang: layar Tayang dan proyektor. Tanpa
    /// tayangan, layar memakai latar umum.
    fn apply_live_background(&self) {
        let live = self.state.borrow().live.as_ref().map(|slide| slide.item);
        let picture = self.picture_for(live);
        let (caption, name, showing) = {
            let st = self.state.borrow();
            let item = live.and_then(|id| st.library.item(id));
            // Sesudah `picture_for`: latar yang hilang sudah dilepas.
            let name = item
                .and_then(|item| item.background.clone())
                .or_else(|| st.library.background_image.clone());
            let caption = item.map_or_else(String::new, |item| {
                let latar = match &name {
                    Some(name) if background::is_video(name) => {
                        format!("video {}", background_label(name))
                    }
                    Some(name) => background_label(name).to_owned(),
                    None => SLIDE_NAMES
                        .get(st.library.background)
                        .copied()
                        .unwrap_or_default()
                        .to_owned(),
                };
                format!("{} · latar {latar}", kind_label(item.kind))
            });
            (caption, name, st.output_visible && !st.blackout)
        };
        self.ui.set_live_caption(caption.into());
        // Video hanya diputar saat proyektor terbuka dan tidak digelapkan.
        let video = name.filter(|name| showing && background::is_video(name));
        let has = picture.is_some();
        if !self.play_video(video) {
            let image = picture.unwrap_or_default();
            self.ui.set_live_picture(image.clone());
            self.ui.set_has_live_picture(has);
            self.output.set_picture(image);
            self.output.set_has_picture(has);
        }
        self.prune_pictures();
    }

    /// Memutar video latar `name` di proyektor, atau menghentikan video yang
    /// sedang diputar (`None`). Mengembalikan `true` bila video itu memang
    /// sudah berputar, supaya frame yang tampil tidak ditimpa gambar diamnya
    /// setiap kali slide berganti.
    fn play_video(&self, name: Option<String>) -> bool {
        let path = name.and_then(|name| background::video_path(&self.profile_dir(), &name).ok());
        let mut current = self.video.borrow_mut();
        if path.is_some() && current.as_ref().map(|(p, _)| p) == path.as_ref() {
            return true;
        }
        // Pemutar lama berhenti sendiri; frame yang masih di jalan diabaikan.
        *current = None;
        let serial = self.video_serial.fetch_add(1, Ordering::SeqCst) + 1;
        let Some(path) = path else {
            return false;
        };
        let (output, ui) = (self.output.as_weak(), self.ui.as_weak());
        let (now, shown) = (self.video_serial.clone(), self.video_frames.clone());
        let busy = Arc::new(AtomicBool::new(false));
        let mut frames = 0u32;
        let deliver = move |frame: SharedPixelBuffer<Rgba8Pixel>| {
            // Tampilan belum selesai dengan frame sebelumnya: buang yang ini.
            if busy.swap(true, Ordering::SeqCst) {
                return false;
            }
            // Layar jemaat di jendela operator kecil; sepertiga frame cukup dan
            // menghemat CPU.
            frames = frames.wrapping_add(1);
            let mirror = frames % 3 == 1;
            let (output, ui) = (output.clone(), ui.clone());
            let (done, now, shown) = (busy.clone(), now.clone(), shown.clone());
            let sent = slint::invoke_from_event_loop(move || {
                done.store(false, Ordering::SeqCst);
                if now.load(Ordering::SeqCst) != serial {
                    return;
                }
                let image = Image::from_rgba8(frame);
                if mirror && let Some(ui) = ui.upgrade() {
                    ui.set_live_picture(image.clone());
                    ui.set_has_live_picture(true);
                }
                if let Some(output) = output.upgrade() {
                    output.set_picture(image);
                    output.set_has_picture(true);
                    shown.fetch_add(1, Ordering::Relaxed);
                }
            });
            if sent.is_err() {
                busy.store(false, Ordering::SeqCst);
            }
            sent.is_ok()
        };
        let (ui, now) = (self.ui.as_weak(), self.video_serial.clone());
        let failed = move |error: String| {
            let _ = slint::invoke_from_event_loop(move || {
                if now.load(Ordering::SeqCst) == serial
                    && let Some(ui) = ui.upgrade()
                {
                    ui.set_status(
                        format!("Video latar tidak bisa diputar: {error} Proyektor memakai gambar diamnya.")
                            .into(),
                    );
                    ui.set_status_ok(false);
                }
            });
        };
        *current = Some((path.clone(), video::play(path, deliver, failed)));
        false
    }

    /// Menghentikan video yang sedang diputar dan menunggu berkasnya dilepas,
    /// sebelum berkas itu dihapus.
    fn stop_video_now(&self) {
        self.video_serial.fetch_add(1, Ordering::SeqCst);
        if let Some((_, player)) = self.video.borrow_mut().take() {
            player.stop_and_wait();
        }
    }

    /// Gambar latar umum. Gambar yang hilang atau rusak dilepas: latar umum
    /// kembali ke warna, dengan pesan.
    fn general_picture(&self) -> Option<Image> {
        let name = self.state.borrow().library.background_image.clone()?;
        match self.picture(&name) {
            Ok(image) => Some(image),
            Err(e) => {
                self.state.borrow_mut().library.background_image = None;
                self.persist();
                self.ui.set_has_bg_picture(false);
                self.status(&format!("{e} Latar umum kembali ke warna."), false);
                None
            }
        }
    }

    /// Gambar latar materi `id`: gambarnya sendiri, atau latar umum. Gambar
    /// sendiri yang hilang atau rusak dilepas dari materi, dengan pesan.
    fn picture_for(&self, id: Option<u64>) -> Option<Image> {
        let own = {
            let st = self.state.borrow();
            id.and_then(|id| st.library.item(id))
                .and_then(|item| item.background.clone())
        };
        if let (Some(id), Some(name)) = (id, own) {
            match self.picture(&name) {
                Ok(image) => return Some(image),
                Err(e) => {
                    self.state
                        .borrow_mut()
                        .library
                        .set_item_background(id, None);
                    self.persist();
                    self.status(&format!("{e} Materi ini memakai latar umum."), false);
                }
            }
        }
        self.general_picture()
    }

    /// Gambar latar yang sudah dibuka, atau dibuka sekarang.
    fn picture(&self, name: &str) -> Result<Image, String> {
        if let Some(image) = self.pictures.borrow().get(name) {
            return Ok(image.clone());
        }
        let dir = self.profile_dir();
        ensure_poster(&dir, name);
        let image = to_image(background::load(&dir, name)?);
        self.pictures
            .borrow_mut()
            .insert(name.to_owned(), image.clone());
        Ok(image)
    }

    /// Melepas gambar yang tidak dipakai lagi dari memori. Satu gambar
    /// 1920×1080 sekitar 6 MB.
    fn prune_pictures(&self) {
        let used: Vec<String> = {
            let st = self.state.borrow();
            let own = |id: Option<u64>| {
                id.and_then(|id| st.library.item(id))
                    .and_then(|item| item.background.clone())
            };
            [
                st.library.background_image.clone(),
                own(st.selected),
                own(st.live.as_ref().map(|slide| slide.item)),
            ]
            .into_iter()
            .flatten()
            .collect()
        };
        self.pictures
            .borrow_mut()
            .retain(|name, _| used.contains(name));
    }

    /// Membuka galeri latar untuk materi `target`, atau untuk latar umum.
    fn show_backgrounds(&self, target: Option<u64>) {
        self.bg_target.set(target);
        self.refresh_background_tiles();
        self.ui.invoke_open_backgrounds();
    }

    /// Galeri latar untuk materi terpilih. Ayat Alkitab disimpan dulu ke
    /// pustaka, seperti saat diedit.
    fn show_item_backgrounds(&self) {
        let Some(id) = self.state.borrow().selected else {
            self.status("Pilih materi yang ingin diberi latar.", false);
            return;
        };
        if Library::is_scratch(id) && (self.materialize(id).is_none() || !self.persist()) {
            return;
        }
        let selected = self.state.borrow().selected;
        self.show_backgrounds(selected);
    }

    /// Mengisi daftar latar. Gambar kecil dibuka sekali per profil; gambar
    /// yang rusak tetap tampil (kosong) supaya bisa dihapus.
    fn refresh_background_tiles(&self) {
        let dir = self.profile_dir();
        let (current, item_title) = {
            let st = self.state.borrow();
            match self.bg_target.get() {
                Some(id) => {
                    let item = st.library.item(id);
                    (
                        item.and_then(|i| i.background.clone()),
                        Some(item.map(|i| i.title.clone()).unwrap_or_default()),
                    )
                }
                None => (st.library.background_image.clone(), None),
            }
        };
        self.ui.set_bg_for_item(item_title.is_some());
        self.ui
            .set_bg_item_title(item_title.unwrap_or_default().into());
        self.ui.set_bg_item_own(current.is_some());
        let names = background::list(&dir);
        let mut thumbs = self.bg_thumbs.borrow_mut();
        thumbs.retain(|name, _| names.contains(name));
        let tiles: Vec<BackgroundTile> = names
            .iter()
            .map(|name| {
                let picture = thumbs
                    .entry(name.clone())
                    .or_insert_with(|| {
                        ensure_poster(&dir, name);
                        background::thumbnail(&dir, name)
                            .map(to_image)
                            .unwrap_or_default()
                    })
                    .clone();
                BackgroundTile {
                    name: name.into(),
                    label: background::label(name).into(),
                    picture,
                    on: current.as_deref() == Some(name),
                    video: background::is_video(name),
                }
            })
            .collect();
        self.background_tiles.set_vec(tiles);
    }

    /// Gambar di galeri diklik: dipakai untuk sasaran galeri (materi atau
    /// latar umum).
    fn pick_background(&self, name: &str) {
        self.use_background(self.bg_target.get(), Some(name.to_owned()));
    }

    /// Materi sasaran galeri kembali memakai latar umum.
    fn use_general_background(&self) {
        if let Some(id) = self.bg_target.get() {
            self.use_background(Some(id), None);
        }
    }

    /// Memasang gambar latar `name` ke materi `target`, atau ke latar umum.
    /// `None` melepas gambar materi (kembali ke latar umum); untuk latar umum
    /// pakai `background_changed`.
    fn use_background(&self, target: Option<u64>, name: Option<String>) {
        let changed = {
            let mut st = self.state.borrow_mut();
            match target {
                Some(id) => st.library.set_item_background(id, name.clone()),
                None if name.is_some() && st.library.background_image != name => {
                    st.library.background_image = name.clone();
                    true
                }
                None => false,
            }
        };
        if !changed {
            return;
        }
        self.persist();
        self.apply_background();
        self.refresh_background_tiles();
        let label = name.as_deref().map(background_label);
        let message = match (target, label) {
            (Some(_), Some(label)) => format!("Latar materi ini: {label}."),
            (Some(_), None) => "Materi ini kembali memakai latar umum.".to_owned(),
            (None, Some(label)) if self.selected_has_own_background() => {
                format!("Latar umum: {label}. Materi ini tetap memakai latarnya sendiri.")
            }
            (None, label) => format!("Latar umum: {}.", label.unwrap_or_default()),
        };
        // Gambar yang gagal dibuka sudah dilepas dengan pesannya sendiri.
        let applied = {
            let st = self.state.borrow();
            match target {
                Some(id) => st.library.item(id).map(|i| &i.background) == Some(&name),
                None => st.library.background_image == name,
            }
        };
        if applied {
            self.status(&message, true);
        }
    }

    fn add_backgrounds(self: &Rc<Self>) {
        if self.adding.borrow().is_some() {
            self.status("Latar masih ditambahkan; tunggu sebentar.", false);
            return;
        }
        let mut picker = rfd::FileDialog::new();
        if video::SUPPORTED {
            let all: Vec<&str> = background::EXTENSIONS
                .iter()
                .chain(&background::VIDEO_EXTENSIONS)
                .copied()
                .collect();
            picker = picker
                .set_title("Tambah gambar atau video latar")
                .add_filter("Gambar atau video", &all)
                .add_filter("Gambar JPG atau PNG", &background::EXTENSIONS)
                .add_filter(
                    "Video MP4, M4V, MOV, WMV, MPG, atau AVI",
                    &background::VIDEO_EXTENSIONS,
                );
        } else {
            picker = picker
                .set_title("Tambah gambar latar")
                .add_filter("Gambar JPG atau PNG", &background::EXTENSIONS);
        }
        let folder = easyworship::image_locations()
            .into_iter()
            .chain(
                easyworship::video_locations()
                    .into_iter()
                    .filter(|_| video::SUPPORTED),
            )
            .next();
        if let Some(dir) = folder {
            picker = picker.set_directory(dir);
        }
        self.pick_files_then(picker, |app, paths| app.add_background_files(paths));
    }

    /// Menambahkan gambar dan video di thread lain, karena memperkecil foto
    /// besar atau membuka video bisa butuh sepersekian detik per berkas di
    /// komputer yang lemah. Kemajuan tampil di bilah status.
    fn add_background_files(self: &Rc<Self>, paths: Vec<PathBuf>) {
        if paths.is_empty() || self.adding.borrow().is_some() {
            return;
        }
        let dir = self.profile_dir();
        let (send, results) = mpsc::channel();
        let total = paths.len();
        std::thread::spawn(move || {
            for path in paths {
                if send.send(add_background_file(&dir, &path)).is_err() {
                    return;
                }
            }
        });
        let profile = self.state.borrow().store.id().to_owned();
        *self.adding.borrow_mut() = Some(Adding {
            results,
            profile,
            target: self.bg_target.get(),
            total,
            added: Vec::new(),
            existing: Vec::new(),
            failed: Vec::new(),
        });
        self.status(&format!("Menambahkan latar… 0 dari {total}"), true);
        let weak = Rc::downgrade(self);
        self.adding_timer
            .start(TimerMode::Repeated, Duration::from_millis(120), move || {
                if let Some(app) = weak.upgrade() {
                    app.adding_tick();
                }
            });
    }

    fn adding_tick(&self) {
        let finished = {
            let mut adding = self.adding.borrow_mut();
            let Some(adding) = adding.as_mut() else {
                self.adding_timer.stop();
                return;
            };
            let mut disconnected = false;
            loop {
                match adding.results.try_recv() {
                    Ok(Ok(Added::New(name))) => adding.added.push(name),
                    Ok(Ok(Added::Existing(name))) => adding.existing.push(name),
                    Ok(Err(e)) => adding.failed.push(e),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            disconnected || adding.done() >= adding.total
        };
        if !finished {
            if let Some(adding) = self.adding.borrow().as_ref() {
                self.ui.set_status(
                    format!("Menambahkan latar… {} dari {}", adding.done(), adding.total).into(),
                );
            }
            return;
        }
        self.adding_timer.stop();
        let Some(adding) = self.adding.borrow_mut().take() else {
            return;
        };
        // Satu gambar yang dipilih langsung dipakai sebagai latar, untuk
        // materi atau latar umum sesuai galeri tempat menambahkannya. Bila
        // profil sudah berganti, gambarnya ada di profil lama.
        let same_profile = self.state.borrow().store.id() == adding.profile;
        if adding.total == 1
            && same_profile
            && let Some(name) = adding.added.first().or(adding.existing.first())
        {
            self.use_background(adding.target, Some(name.clone()));
        }
        self.refresh_background_tiles();
        let mut parts = Vec::new();
        if !adding.added.is_empty() {
            parts.push(format!("{} latar ditambahkan", count(adding.added.len())));
        }
        if !adding.existing.is_empty() {
            parts.push(format!("{} sudah ada", count(adding.existing.len())));
        }
        if !adding.failed.is_empty() {
            parts.push(format!("{} gagal", count(adding.failed.len())));
        }
        self.status(&format!("{}.", parts.join(", ")), adding.failed.is_empty());
        if !adding.failed.is_empty() {
            let mut text = adding
                .failed
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            if adding.failed.len() > 5 {
                text.push_str(&format!("\n… dan {} lainnya.", adding.failed.len() - 5));
            }
            dialog(
                Some(self.ui.window()),
                MessageLevel::Warning,
                "Sebagian latar tidak bisa ditambahkan",
                &text,
                MessageButtons::Ok,
            );
        }
    }

    fn remove_background(&self, name: &str) {
        let (general, items) = {
            let st = self.state.borrow();
            (
                st.library.background_image.as_deref() == Some(name),
                st.library.background_users(name),
            )
        };
        let (what, which) = if background::is_video(name) {
            ("video", "Video")
        } else {
            ("gambar", "Gambar")
        };
        let mut text = format!(
            "Hapus \"{}\" dari latar profil ini?\n\nBerkas {what} asli di komputer tidak ikut terhapus.",
            background_label(name)
        );
        if general {
            text.push_str(&format!(
                "\n{which} ini latar umum; latar umum kembali ke warna."
            ));
        }
        if items > 0 {
            text.push_str(&format!(
                "\n{which} ini latar {} materi; materi itu kembali memakai latar umum.",
                count(items)
            ));
        }
        let answer = dialog(
            Some(self.ui.window()),
            MessageLevel::Warning,
            "Hapus latar",
            &text,
            MessageButtons::YesNo,
        );
        if answer == MessageDialogResult::Yes {
            self.remove_background_now(name);
        }
    }

    fn remove_background_now(&self, name: &str) {
        // Windows tidak bisa menghapus video yang sedang diputar.
        let dir = self.profile_dir();
        if background::is_video(name)
            && self.video.borrow().as_ref().map(|(path, _)| path)
                == background::video_path(&dir, name).ok().as_ref()
        {
            self.stop_video_now();
        }
        if let Err(e) = background::remove(&dir, name) {
            self.status(&e, false);
            return;
        }
        self.bg_thumbs.borrow_mut().remove(name);
        self.pictures.borrow_mut().remove(name);
        let changed = {
            let mut st = self.state.borrow_mut();
            let general = st.library.background_image.as_deref() == Some(name);
            if general {
                st.library.background_image = None;
            }
            st.library.forget_item_background(name) > 0 || general
        };
        if changed {
            self.persist();
            self.apply_background();
        }
        self.refresh_background_tiles();
        self.status(&format!("Latar {} dihapus.", background_label(name)), true);
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
                let image = self.state.borrow().library.background_image.clone();
                let from = self.profile_dir();
                created.map(|mut opened| {
                    if let Some(image) = image
                        && let Some(to) = opened.store.path().parent()
                    {
                        // Gagal menyalin tidak menggagalkan profil baru; latarnya
                        // cukup kembali ke warna.
                        opened.library.background_image = background::copy(&from, to, &image).ok();
                    }
                    self.use_profile(opened)
                })
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
                    self.sync_now();
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
                self.sync_profile_removed(id);
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
        self.ui.set_editor_dirty(false);
        self.ui.set_query(SharedString::new());
        self.ui.set_bg_index(background);
        self.output.set_bg_index(background);
        // Gambar latar ada di folder profil, dan id materi hanya berlaku di
        // profil itu.
        self.pictures.borrow_mut().clear();
        self.bg_thumbs.borrow_mut().clear();
        self.bg_target.set(None);
        self.apply_background();
        self.persist();
        self.refresh_library();
        self.refresh_service();
        self.refresh_live();
        self.select_first();
        self.refresh_profile_label();
        self.close_profiles();
        let name = self.state.borrow().store.name().to_owned();
        self.status(&format!("Profil {name} dibuka."), true);
        self.sync_now();
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
            hold_until: None,
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

    /// Melompati langkah uji sampai `last`, misalnya uji video tanpa video uji.
    fn skip_smoke_steps(&self, last: u32) {
        if let Some(smoke) = self.smoke.borrow_mut().as_mut() {
            smoke.step = last + 1;
        }
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
                self.check("bible_view", self.ui.get_bible_view());
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
                // Referensi diketik di kotak cari, lalu Enter.
                self.ui.set_query("yoh 3:16-18".into());
                self.search_changed();
                let hinted = self.ui.get_query_reference() == "Yohanes 3:16-18"
                    && self.ui.get_left_tab() == 0
                    && !self.ui.get_bible_view();
                self.search_accepted();
                self.check(
                    "search_opens_bible",
                    hinted
                        && self.ui.get_query().is_empty()
                        && self.ui.get_left_tab() == 1
                        && self.ui.get_bible_view()
                        && self.ui.get_bible_ref() == "yoh 3:16-18",
                );
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
                // Referensi ayat di bawah teks di layar proyektor.
                let projected = self.output.get_tile();
                self.check(
                    "projector_caption",
                    projected.verse
                        && projected.caption.starts_with("Yohanes 3:16")
                        && projected.caption.ends_with(" (UJI)"),
                );
                self.check(
                    "bible_history",
                    self.ui.get_bible_history().row_count() == 1
                        && self
                            .ui
                            .get_bible_history()
                            .row_data(0)
                            .is_some_and(|row| row.label == "Yohanes 3:16-20"),
                );
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
                self.ui.invoke_close_about();
                // Gambar latar: dibuat di folder data uji, lalu ditambahkan
                // lewat jalur yang sama dengan tombol Tambah gambar.
                let source = self.data_dir.join("Latar uji.png");
                let made = image::RgbImage::from_fn(1600, 900, |x, y| {
                    image::Rgb([(x / 7) as u8, (y / 4) as u8, 200 - (x / 9) as u8])
                })
                .save(&source);
                self.check("background_source", made.is_ok());
                self.add_background_files(vec![source]);
            }
            20 => {
                if self.adding.borrow().is_some() && self.retry() {
                    return;
                }
                let name = "Latar uji.jpg";
                let saved = self
                    .state
                    .borrow()
                    .store
                    .load()
                    .is_ok_and(|l| l.background_image.as_deref() == Some(name));
                let listed = background::list(&self.profile_dir()) == [name];
                self.check(
                    "background_added",
                    saved
                        && listed
                        && self.ui.get_has_bg_picture()
                        && self.output.get_has_picture(),
                );
                self.show_backgrounds(None);
            }
            21 => {
                self.snapshot("ui-smoke-latar.png", self.ui.window());
                self.check(
                    "background_listed",
                    self.background_tiles.row_count() == 1
                        && self.background_tiles.row_data(0).is_some_and(|t| t.on),
                );
                self.ui.invoke_close_backgrounds();
            }
            22 => {
                self.snapshot("ui-smoke-latar-slide.png", self.ui.window());
                self.snapshot("ui-smoke-latar-proyektor.png", self.output.window());
                // Latar materi: latar umum warna, materi terpilih memakai gambar
                // sendiri, lalu materi itu ditayangkan.
                self.background_changed(1);
                self.show_item_backgrounds();
                self.pick_background("Latar uji.jpg");
                let selected = self.state.borrow().selected;
                let own = |l: &Library| {
                    selected
                        .and_then(|id| l.item(id))
                        .and_then(|i| i.background.clone())
                };
                let name = Some("Latar uji.jpg".to_owned());
                let saved = self
                    .state
                    .borrow()
                    .store
                    .load()
                    .is_ok_and(|l| own(&l) == name && l.background_image.is_none());
                let picked = own(&self.state.borrow().library) == name;
                let gallery = self.ui.get_bg_for_item()
                    && self.background_tiles.row_data(0).is_some_and(|t| t.on);
                self.go_live();
                self.check(
                    "item_background",
                    saved
                        && picked
                        && gallery
                        && self.ui.get_current_background() == "Latar uji"
                        && self.ui.get_has_view_picture()
                        && !self.ui.get_has_bg_picture()
                        && self.ui.get_has_live_picture()
                        && self.output.get_has_picture(),
                );
            }
            23 => {
                self.snapshot("ui-smoke-latar-materi-galeri.png", self.ui.window());
                self.ui.invoke_close_backgrounds();
            }
            24 => {
                self.snapshot("ui-smoke-latar-materi.png", self.ui.window());
                self.snapshot("ui-smoke-latar-materi-proyektor.png", self.output.window());
                // Kembali ke latar umum (warna), lalu pasang lagi untuk uji hapus.
                self.use_general_background();
                self.check(
                    "item_background_general",
                    !self.ui.get_has_view_picture()
                        && !self.ui.get_has_live_picture()
                        && !self.output.get_has_picture()
                        && !self.selected_has_own_background(),
                );
                let name = "Latar uji.jpg";
                let selected = self.state.borrow().selected;
                self.use_background(selected, Some(name.to_owned()));
                self.use_background(None, Some(name.to_owned()));
                let both = self.selected_has_own_background() && self.ui.get_has_bg_picture();
                // Menghapus gambar melepasnya dari latar umum dan dari materi.
                self.remove_background_now(name);
                let library = self.state.borrow().library.clone();
                self.check(
                    "background_removed",
                    both && background::list(&self.profile_dir()).is_empty()
                        && !self.output.get_has_picture()
                        && !self.ui.get_has_view_picture()
                        && library.background_image.is_none()
                        && library.background_users(name) == 0
                        && self.pictures.borrow().is_empty(),
                );
            }
            // Video latar, hanya bila LENTERA_SMOKE_VIDEO menunjuk ke video uji.
            // Repositori tidak menyimpan video, dan tidak setiap Windows (misalnya
            // Windows Server di CI) punya Media Foundation.
            25 => {
                let Some(source) = smoke_video() else {
                    self.skip_smoke_steps(29);
                    return;
                };
                if !self.state.borrow().output_visible {
                    self.show_output();
                }
                self.bg_target.set(None);
                self.add_background_files(vec![source]);
            }
            26 => {
                if self.adding.borrow().is_some() && self.retry() {
                    return;
                }
                let general = self.state.borrow().library.background_image.clone();
                let name = general.clone().unwrap_or_default();
                let dir = self.profile_dir();
                self.show_backgrounds(None);
                self.check(
                    "video_added",
                    background::is_video(&name)
                        && background::list(&dir) == [name.clone()]
                        && background::has_poster(&dir, &name)
                        && self.ui.get_has_bg_picture()
                        && self
                            .background_tiles
                            .row_data(0)
                            .is_some_and(|t| t.video && t.on),
                );
                self.video_frames.store(0, Ordering::SeqCst);
            }
            27 => {
                // Proyektor terbuka: video berputar dan frame-nya tampil.
                let frames = self.video_frames.load(Ordering::SeqCst);
                if frames < 15 && self.retry() {
                    return;
                }
                self.snapshot("ui-smoke-video-galeri.png", self.ui.window());
                self.ui.invoke_close_backgrounds();
                self.check(
                    "video_playing",
                    self.video.borrow().is_some()
                        && frames >= 15
                        && self.output.get_has_picture()
                        && self.ui.get_live_caption().contains("latar video"),
                );
                // Slide berganti tanpa memulai video dari awal.
                let serial = self.video_serial.load(Ordering::SeqCst);
                self.navigate(true);
                self.check(
                    "video_keeps_playing",
                    self.video_serial.load(Ordering::SeqCst) == serial
                        && self.video.borrow().is_some(),
                );
            }
            28 => {
                // LENTERA_SMOKE_HOLD=<detik>: video diputar lebih lama di sini,
                // untuk mengukur RAM dan CPU dari luar.
                if let Some(hold) = std::env::var("LENTERA_SMOKE_HOLD")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    let mut smoke = self.smoke.borrow_mut();
                    if let Some(smoke) = smoke.as_mut() {
                        let until = *smoke
                            .hold_until
                            .get_or_insert_with(|| Instant::now() + Duration::from_secs(hold));
                        if Instant::now() < until {
                            smoke.step -= 1;
                            return;
                        }
                    }
                }
                self.snapshot("ui-smoke-video-proyektor.png", self.output.window());
                // Gelapkan menghentikan video; memulihkan memutarnya lagi.
                self.toggle_blackout();
                let stopped = self.video.borrow().is_none();
                self.toggle_blackout();
                self.check("video_blackout", stopped && self.video.borrow().is_some());
            }
            29 => {
                // Video yang sedang diputar bisa dihapus: pemutarnya dihentikan
                // dulu supaya Windows melepas berkasnya.
                let name = self
                    .state
                    .borrow()
                    .library
                    .background_image
                    .clone()
                    .unwrap_or_default();
                let dir = self.profile_dir();
                self.remove_background_now(&name);
                self.check(
                    "video_removed",
                    background::list(&dir).is_empty()
                        && !background::folder(&dir).join(&name).exists()
                        && self.video.borrow().is_none()
                        && !self.output.get_has_picture(),
                );
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

/// Bekal membuka jendela operator, disiapkan sebelum layar masuk.
struct Launch {
    dir: PathBuf,
    lock: DataLock,
    smoke: bool,
    update_source: Option<String>,
}

fn run(args: &Args) -> Result<(), String> {
    if args.smoke && args.data_dir.is_none() {
        return Err("Pengujian UI wajib memakai --data-dir terpisah.".into());
    }
    let dir = args.data_dir.clone().unwrap_or_else(default_data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Folder data tidak bisa dibuat: {e}"))?;
    let lock = DataLock::acquire(&dir.join(LOCK_FILE))?;

    let backend = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into());
    // Ikon jendela sejak dibuat (operator, proyektor, dan layar masuk).
    // Ukuran pastinya dipasang lagi oleh `window_icons` setelah skala layar
    // diketahui.
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

    let launch = Launch {
        dir: dir.clone(),
        lock,
        smoke: args.smoke,
        update_source: args.update_source.clone(),
    };
    // Jendela operator; dibuat sekarang, atau setelah masuk ke akun.
    let app: Rc<RefCell<Option<Rc<App>>>> = Rc::default();
    let mut login = None;
    match akun::server(args.server.as_deref(), args.smoke) {
        None => *app.borrow_mut() = Some(open_app(launch)?),
        Some(server) => match akun::saved_account(&server, &dir) {
            Some(account) => {
                let opened = open_app(launch)?;
                opened.start_sync(account);
                *app.borrow_mut() = Some(opened);
            }
            None => login = Some(akun::sign_in_first(server, launch, app.clone())?),
        },
    }

    slint::run_event_loop().map_err(|e| e.to_string())?;
    drop(login);
    // Melepas kunci folder data sebelum pembaruan dipasang.
    drop(app);
    Ok(())
}

/// Membuka profil terakhir dan menampilkan jendela operator.
fn open_app(launch: Launch) -> Result<Rc<App>, String> {
    let Launch {
        dir,
        lock,
        smoke,
        update_source,
    } = launch;
    // Profil pertama; dibuat (dan `library.json` lama dipindahkan) bila belum ada.
    let Opened {
        store,
        library,
        fresh: is_new,
    } = Store::open(&dir)?;

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
    if !smoke && app.refresh_profiles() > 1 {
        app.show_profiles();
    }
    app.ui.show().map_err(|e| e.to_string())?;

    let weak = Rc::downgrade(&app);
    let smoke = smoke.then(|| dir.join("ui-smoke-result.json"));
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
            None => app.check_updates(update_source),
        }
    });
    Ok(app)
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
