#![windows_subsystem = "windows"]
#![allow(unsafe_op_in_unsafe_fn)]

use lentera_core::{Cursor, DataLock, Kind, LIBRARY_FILE, Library, Slide, Step, default_data_dir};
use std::{
    cell::RefCell,
    collections::HashMap,
    mem::{size_of, zeroed},
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::Dialogs::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*,
        WindowsAndMessaging::*,
    },
};

const SEARCH: u16 = 101;
const FILTER: u16 = 102;
const LIBRARY: u16 = 103;
const TITLE: u16 = 104;
const BODY: u16 = 105;
const NEW: u16 = 106;
const SAVE: u16 = 107;
const DELETE: u16 = 108;
const IMPORT_TXT: u16 = 109;
const IMPORT_BIBLE: u16 = 110;
const SERVICE: u16 = 111;
const ADD: u16 = 112;
const UP: u16 = 113;
const DOWN: u16 = 114;
const REMOVE: u16 = 115;
const SLIDES: u16 = 116;
const PREVIEW: u16 = 117;
const LIVE_PREVIEW: u16 = 118;
const LIVE: u16 = 119;
const BLACKOUT: u16 = 120;
const PROJECTOR: u16 = 121;
const PREVIOUS: u16 = 122;
const NEXT: u16 = 123;
const MONITOR: u16 = 124;
const BACKGROUND: u16 = 125;
const STATUS: u16 = 126;
const SAVE_TIMER: usize = 1;
const SMOKE_TIMER: usize = 2;
const SS_LEFT: u32 = 0;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
const SURFACE: u32 = rgb(244, 247, 250);
const INK: u32 = rgb(25, 43, 59);
const ACCENT: u32 = rgb(0, 114, 119);

thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }

#[derive(Clone)]
struct Monitor {
    rect: RECT,
    primary: bool,
}
struct App {
    _data_lock: DataLock,
    hwnd: HWND,
    output: HWND,
    controls: HashMap<u16, HWND>,
    font: HFONT,
    library: Library,
    path: PathBuf,
    filtered: Vec<u64>,
    selected: Option<u64>,
    page: usize,
    live: Option<Slide>,
    live_service: Option<usize>,
    blackout: bool,
    fullscreen: bool,
    dirty: bool,
    monitors: Vec<Monitor>,
    smoke_step: Option<u8>,
    smoke_report: PathBuf,
    smoke_checks: Vec<(String, bool)>,
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut()
            && let Some(app) = state.as_mut()
        {
            f(app);
        }
    });
}

unsafe fn text(hwnd: HWND) -> String {
    let size = GetWindowTextLengthW(hwnd).max(0) as usize;
    let mut buffer = vec![0u16; size + 1];
    let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied])
}
unsafe fn set_text(hwnd: HWND, value: &str) {
    SetWindowTextW(hwnd, wide(value).as_ptr());
}
unsafe fn message(hwnd: HWND, title: &str, value: &str, flags: u32) -> i32 {
    MessageBoxW(hwnd, wide(value).as_ptr(), wide(title).as_ptr(), flags)
}

unsafe extern "system" fn enumerate_monitor(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> i32 {
    let mut info: MONITORINFO = zeroed();
    info.cbSize = size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut info) != 0 {
        (*(data as *mut Vec<Monitor>)).push(Monitor {
            rect: info.rcMonitor,
            primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    1
}

impl App {
    unsafe fn new(hwnd: HWND, path: PathBuf, smoke: bool) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data_lock = DataLock::acquire(&path.with_extension("lock"))?;
        let is_new = !path.exists();
        let mut library = Library::load(&path)?;
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let font = CreateFontW(
            -(10 * dpi + 36) / 72,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        let mut monitors = Vec::new();
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(enumerate_monitor),
            &mut monitors as *mut _ as LPARAM,
        );
        monitors.sort_by_key(|m: &Monitor| !m.primary);
        if is_new && monitors.len() > 1 {
            library.monitor = 1;
        }
        let smoke_report = path.with_file_name("ui-smoke-result.json");
        let mut app = Self {
            _data_lock: data_lock,
            hwnd,
            output: null_mut(),
            controls: HashMap::new(),
            font,
            library,
            path,
            filtered: Vec::new(),
            selected: None,
            page: 0,
            live: None,
            live_service: None,
            blackout: false,
            fullscreen: false,
            dirty: false,
            monitors,
            smoke_step: smoke.then_some(0),
            smoke_report,
            smoke_checks: Vec::new(),
        };
        app.create_controls();
        app.refresh_library();
        app.refresh_service();
        if let Some(id) = app.filtered.first().copied() {
            app.select(id);
        }
        app.status("Siap. Pilih materi, lalu tekan F5 untuk menayangkan.");
        Ok(app)
    }
    fn c(&self, id: u16) -> HWND {
        self.controls.get(&id).copied().unwrap_or(null_mut())
    }
    unsafe fn make(&mut self, id: u16, class: &str, value: &str, style: u32, ex: u32) {
        let hwnd = CreateWindowExW(
            ex,
            wide(class).as_ptr(),
            wide(value).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            10,
            10,
            self.hwnd,
            id as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(hwnd, WM_SETFONT, self.font as WPARAM, 1);
        self.controls.insert(id, hwnd);
    }
    unsafe fn create_controls(&mut self) {
        for (id, value) in [
            (201, "PUSTAKA"),
            (202, "SUSUNAN IBADAH"),
            (203, "PRATINJAU"),
            (204, "EDITOR MATERI"),
            (205, "BAGIAN / SLIDE"),
            (206, "SEDANG TAYANG"),
            (207, "Monitor keluaran"),
            (208, "Latar layar"),
        ] {
            self.make(id, "STATIC", value, SS_LEFT, 0);
        }
        self.make(
            SEARCH,
            "EDIT",
            "",
            WS_TABSTOP | ES_AUTOHSCROLL as u32,
            WS_EX_CLIENTEDGE,
        );
        SendMessageW(
            self.c(SEARCH),
            EM_SETCUEBANNER,
            1,
            wide("Cari judul, referensi, atau isi...").as_ptr() as LPARAM,
        );
        self.make(
            FILTER,
            "COMBOBOX",
            "",
            WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
            0,
        );
        for label in ["Semua materi", "Lirik lagu", "Ayat Alkitab"] {
            SendMessageW(
                self.c(FILTER),
                CB_ADDSTRING,
                0,
                wide(label).as_ptr() as LPARAM,
            );
        }
        SendMessageW(self.c(FILTER), CB_SETCURSEL, 0, 0);
        self.make(
            LIBRARY,
            "LISTBOX",
            "",
            WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32,
            WS_EX_CLIENTEDGE,
        );
        self.make(
            SERVICE,
            "LISTBOX",
            "",
            WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32,
            WS_EX_CLIENTEDGE,
        );
        self.make(
            SLIDES,
            "LISTBOX",
            "",
            WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32,
            WS_EX_CLIENTEDGE,
        );
        self.make(
            TITLE,
            "EDIT",
            "",
            WS_TABSTOP | ES_AUTOHSCROLL as u32,
            WS_EX_CLIENTEDGE,
        );
        SendMessageW(
            self.c(TITLE),
            EM_SETCUEBANNER,
            1,
            wide("Judul lagu / referensi ayat").as_ptr() as LPARAM,
        );
        self.make(
            BODY,
            "EDIT",
            "",
            WS_TABSTOP
                | WS_VSCROLL
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_WANTRETURN as u32,
            WS_EX_CLIENTEDGE,
        );
        SendMessageW(self.c(BODY), EM_SETLIMITTEXT, 200_000, 0);
        SendMessageW(self.c(TITLE), EM_SETLIMITTEXT, 500, 0);
        for (id, value) in [
            (NEW, "Baru"),
            (SAVE, "Simpan"),
            (DELETE, "Hapus"),
            (IMPORT_TXT, "Impor lirik .txt"),
            (IMPORT_BIBLE, "Impor ayat .tsv"),
            (ADD, "Tambah ke susunan →"),
            (UP, "Naik"),
            (DOWN, "Turun"),
            (REMOVE, "Lepas"),
            (BLACKOUT, "Gelapkan [F6]"),
            (PROJECTOR, "Buka proyektor"),
            (PREVIOUS, "← Sebelumnya"),
            (NEXT, "Berikutnya →"),
        ] {
            self.make(id, "BUTTON", value, WS_TABSTOP | BS_PUSHBUTTON as u32, 0);
        }
        self.make(
            LIVE,
            "BUTTON",
            "Tayangkan [F5]",
            WS_TABSTOP | BS_OWNERDRAW as u32,
            0,
        );
        self.make(PREVIEW, "LenteraSlide", "", WS_BORDER, 0);
        self.make(LIVE_PREVIEW, "LenteraSlide", "", WS_BORDER, 0);
        self.make(
            MONITOR,
            "COMBOBOX",
            "",
            WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
            0,
        );
        for (index, m) in self.monitors.iter().enumerate() {
            let label = format!(
                "{} · {} × {}{}",
                index + 1,
                m.rect.right - m.rect.left,
                m.rect.bottom - m.rect.top,
                if m.primary { " · utama" } else { "" }
            );
            SendMessageW(
                self.c(MONITOR),
                CB_ADDSTRING,
                0,
                wide(&label).as_ptr() as LPARAM,
            );
        }
        self.library.monitor = self
            .library
            .monitor
            .min(self.monitors.len().saturating_sub(1));
        SendMessageW(self.c(MONITOR), CB_SETCURSEL, self.library.monitor, 0);
        self.make(
            BACKGROUND,
            "COMBOBOX",
            "",
            WS_TABSTOP | CBS_DROPDOWNLIST as u32,
            0,
        );
        for label in ["Gelap", "Biru malam", "Terang"] {
            SendMessageW(
                self.c(BACKGROUND),
                CB_ADDSTRING,
                0,
                wide(label).as_ptr() as LPARAM,
            );
        }
        self.library.background = self.library.background.min(2);
        SendMessageW(self.c(BACKGROUND), CB_SETCURSEL, self.library.background, 0);
        self.make(STATUS, "STATIC", "", SS_LEFT, 0);
    }
    unsafe fn place(&self, id: u16, x: i32, y: i32, w: i32, h: i32) {
        let scale = GetDpiForWindow(self.hwnd).max(96) as f64 / 96.;
        SetWindowPos(
            self.c(id),
            null_mut(),
            (x as f64 * scale) as i32,
            (y as f64 * scale) as i32,
            (w.max(1) as f64 * scale) as i32,
            (h.max(1) as f64 * scale) as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    unsafe fn layout(&self) {
        let mut rect: RECT = zeroed();
        GetClientRect(self.hwnd, &mut rect);
        let scale = GetDpiForWindow(self.hwnd).max(96) as f64 / 96.;
        let w = (rect.right as f64 / scale) as i32;
        let h = (rect.bottom as f64 / scale) as i32;
        let left = ((w - 50) * 27 / 100).clamp(220, 320);
        let middle = ((w - 50) * 23 / 100).clamp(180, 270);
        let x2 = left + 32;
        let x3 = x2 + middle + 16;
        let right = w - x3 - 16;
        self.place(SEARCH, 16, 66, (w - 470).max(180), 30);
        self.place(FILTER, (w - 444).max(205), 66, 140, 180);
        self.place(IMPORT_TXT, w - 294, 66, 134, 30);
        self.place(IMPORT_BIBLE, w - 152, 66, 136, 30);
        self.place(201, 16, 112, left, 20);
        self.place(202, x2, 112, middle, 20);
        self.place(203, x3, 112, right, 20);
        let lib_height = ((h - 250) / 2).max(120);
        self.place(LIBRARY, 16, 138, left, lib_height);
        self.place(204, 16, 150 + lib_height, left, 20);
        self.place(TITLE, 16, 176 + lib_height, left, 28);
        self.place(
            BODY,
            16,
            212 + lib_height,
            left,
            (h - lib_height - 305).max(80),
        );
        let third = (left - 12) / 3;
        self.place(NEW, 16, h - 84, third, 30);
        self.place(SAVE, 22 + third, h - 84, third, 30);
        self.place(DELETE, 28 + third * 2, h - 84, left - third * 2 - 12, 30);
        let schedule_height = ((h - 290) / 2).max(100);
        self.place(SERVICE, x2, 138, middle, schedule_height);
        self.place(ADD, x2, 146 + schedule_height, middle, 30);
        let button = (middle - 12) / 3;
        self.place(UP, x2, 182 + schedule_height, button, 28);
        self.place(DOWN, x2 + button + 6, 182 + schedule_height, button, 28);
        self.place(
            REMOVE,
            x2 + 2 * button + 12,
            182 + schedule_height,
            button,
            28,
        );
        self.place(205, x2, 224 + schedule_height, middle, 20);
        self.place(
            SLIDES,
            x2,
            250 + schedule_height,
            middle,
            (h - schedule_height - 304).max(70),
        );
        let preview_height = (right * 9 / 16).min(((h - 430) / 2).max(75));
        self.place(PREVIEW, x3, 138, right, preview_height);
        self.place(206, x3, 150 + preview_height, right, 20);
        self.place(
            LIVE_PREVIEW,
            x3,
            176 + preview_height,
            right,
            preview_height,
        );
        let config_y = 190 + preview_height * 2;
        self.place(207, x3, config_y, right * 3 / 5, 20);
        self.place(208, x3 + right * 3 / 5 + 8, config_y, right * 2 / 5 - 8, 20);
        self.place(MONITOR, x3, config_y + 24, right * 3 / 5, 180);
        self.place(
            BACKGROUND,
            x3 + right * 3 / 5 + 8,
            config_y + 24,
            right * 2 / 5 - 8,
            160,
        );
        let half = (right - 8) / 2;
        self.place(PREVIOUS, x3, h - 160, half, 30);
        self.place(NEXT, x3 + half + 8, h - 160, half, 30);
        self.place(LIVE, x3, h - 122, half, 36);
        self.place(BLACKOUT, x3 + half + 8, h - 122, half, 36);
        self.place(PROJECTOR, x3, h - 78, right, 30);
        self.place(STATUS, 16, h - 32, w - 32, 22);
        InvalidateRect(self.hwnd, null(), 0);
    }
    unsafe fn status(&self, value: &str) {
        set_text(self.c(STATUS), value);
    }
    fn kind_filter(&self) -> Option<Kind> {
        match unsafe { SendMessageW(self.c(FILTER), CB_GETCURSEL, 0, 0) } {
            1 => Some(Kind::Song),
            2 => Some(Kind::Verse),
            _ => None,
        }
    }
    unsafe fn refresh_library(&mut self) {
        let query = text(self.c(SEARCH));
        let matches = self.library.search(self.kind_filter(), &query);
        self.filtered = matches.iter().map(|i| i.id).collect();
        SendMessageW(self.c(LIBRARY), WM_SETREDRAW, 0, 0);
        SendMessageW(self.c(LIBRARY), LB_RESETCONTENT, 0, 0);
        for item in matches {
            let label = format!(
                "{}  {}",
                if item.kind == Kind::Song {
                    "♪"
                } else {
                    "▤"
                },
                item.title
            );
            SendMessageW(
                self.c(LIBRARY),
                LB_ADDSTRING,
                0,
                wide(&label).as_ptr() as LPARAM,
            );
        }
        if let Some(index) = self
            .filtered
            .iter()
            .position(|id| Some(*id) == self.selected)
        {
            SendMessageW(self.c(LIBRARY), LB_SETCURSEL, index, 0);
        }
        SendMessageW(self.c(LIBRARY), WM_SETREDRAW, 1, 0);
        InvalidateRect(self.c(LIBRARY), null(), 1);
    }
    unsafe fn refresh_service(&self) {
        let old = SendMessageW(self.c(SERVICE), LB_GETCURSEL, 0, 0);
        SendMessageW(self.c(SERVICE), LB_RESETCONTENT, 0, 0);
        for (index, id) in self.library.service.iter().enumerate() {
            if let Some(item) = self.library.item(*id) {
                SendMessageW(
                    self.c(SERVICE),
                    LB_ADDSTRING,
                    0,
                    wide(&format!("{:02}  {}", index + 1, item.title)).as_ptr() as LPARAM,
                );
            }
        }
        if old >= 0 && !self.library.service.is_empty() {
            SendMessageW(
                self.c(SERVICE),
                LB_SETCURSEL,
                (old as usize).min(self.library.service.len() - 1),
                0,
            );
        }
    }
    unsafe fn refresh_slides(&mut self) {
        SendMessageW(self.c(SLIDES), LB_RESETCONTENT, 0, 0);
        if let Some(item) = self.selected.and_then(|id| self.library.item(id)) {
            let slides = item.slides();
            self.page = self.page.min(slides.len().saturating_sub(1));
            for (index, slide) in slides.iter().enumerate() {
                let excerpt: String = slide
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(42)
                    .collect();
                SendMessageW(
                    self.c(SLIDES),
                    LB_ADDSTRING,
                    0,
                    wide(&format!("{}  {}", index + 1, excerpt)).as_ptr() as LPARAM,
                );
            }
            SendMessageW(self.c(SLIDES), LB_SETCURSEL, self.page, 0);
        }
        InvalidateRect(self.c(PREVIEW), null(), 0);
    }
    unsafe fn select(&mut self, id: u64) {
        self.selected = Some(id);
        self.page = 0;
        if let Some(item) = self.library.item(id) {
            set_text(self.c(TITLE), &item.title);
            set_text(self.c(BODY), &item.content.replace('\n', "\r\n"));
        }
        self.dirty = false;
        self.refresh_slides();
        if let Some(index) = self.filtered.iter().position(|i| *i == id) {
            SendMessageW(self.c(LIBRARY), LB_SETCURSEL, index, 0);
        }
    }
    unsafe fn persist(&self) -> bool {
        match self.library.save(&self.path) {
            Ok(()) => true,
            Err(e) => {
                message(
                    self.hwnd,
                    "Penyimpanan gagal",
                    &format!("Perubahan belum tersimpan.\n{e}"),
                    MB_OK | MB_ICONERROR,
                );
                false
            }
        }
    }
    unsafe fn commit_editor(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        if let Some(id) = self.selected {
            self.library
                .update(id, &text(self.c(TITLE)), &text(self.c(BODY)));
        }
        if !self.persist() {
            return false;
        }
        self.dirty = false;
        self.refresh_library();
        self.refresh_service();
        self.refresh_slides();
        true
    }
    unsafe fn refresh_live(&self) {
        if !self.output.is_null() {
            InvalidateRect(self.output, null(), 0);
        }
        InvalidateRect(self.c(LIVE_PREVIEW), null(), 0);
        set_text(
            self.c(BLACKOUT),
            if self.blackout {
                "Pulihkan [F6]"
            } else {
                "Gelapkan [F6]"
            },
        );
        let label = if self.blackout {
            "SEDANG TAYANG · LAYAR GELAP".into()
        } else if let Some(slide) = &self.live {
            format!(
                "TAYANG · {} · {}/{}",
                slide.title,
                slide.page + 1,
                slide.total
            )
        } else {
            "SEDANG TAYANG · BELUM ADA MATERI".into()
        };
        set_text(self.c(206), &label);
    }
    unsafe fn go_live(&mut self) {
        if !self.commit_editor() {
            return;
        }
        if let Some(id) = self.selected {
            if let Some(slide) = self.library.slide(id, self.page) {
                let sel = SendMessageW(self.c(SERVICE), LB_GETCURSEL, 0, 0);
                self.live_service = self.library.service_position(id, usize::try_from(sel).ok());
                self.live = Some(slide);
                self.blackout = false;
                self.show_output();
                self.refresh_live();
                self.status(
                    "Tayang. PageDown / PageUp: pindah bagian. F6: gelapkan. F11: layar penuh.",
                );
            } else {
                self.status("Materi masih kosong. Isi teks terlebih dahulu.");
            }
        }
    }
    unsafe fn navigate(&mut self, forward: bool) {
        if !self.commit_editor() {
            return;
        }
        let Some(current) = self.live.clone() else {
            self.go_live();
            return;
        };
        let from = Cursor {
            item: current.item,
            page: current.page,
            service: self.live_service,
        };
        let to = match self.library.step(from, forward) {
            Step::Moved(to) => to,
            Step::EndOfItem => {
                self.status(
                    "Sudah di batas materi. Tambahkan materi ke susunan untuk berpindah otomatis.",
                );
                return;
            }
            Step::EndOfService => {
                self.status("Sudah di awal / akhir susunan ibadah.");
                return;
            }
        };
        if let Some(position) = to.service
            && to.service != from.service
        {
            self.live_service = to.service;
            SendMessageW(self.c(SERVICE), LB_SETCURSEL, position, 0);
        }
        self.select(to.item);
        self.page = to.page;
        self.refresh_slides();
        if let Some(slide) = self.library.slide(to.item, self.page) {
            self.live = Some(slide);
        }
        self.refresh_live();
    }
    unsafe fn show_output(&mut self) {
        if self.output.is_null() {
            self.output = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                wide("LenteraOutput").as_ptr(),
                wide("Lentera · Proyektor").as_ptr(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                960,
                600,
                self.hwnd,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
        }
        if IsWindowVisible(self.output) == 0 {
            self.fullscreen = self.monitors.len() > 1;
            self.position_output();
            ShowWindow(self.output, SW_SHOWNOACTIVATE);
            set_text(self.c(PROJECTOR), "Sembunyikan proyektor");
        }
    }
    unsafe fn position_output(&self) {
        let rect = self
            .monitors
            .get(self.library.monitor)
            .map(|m| m.rect)
            .unwrap_or(RECT {
                left: 80,
                top: 80,
                right: 1040,
                bottom: 680,
            });
        if self.fullscreen {
            SetWindowLongPtrW(self.output, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as isize);
            SetWindowPos(
                self.output,
                HWND_TOP,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        } else {
            SetWindowLongPtrW(self.output, GWL_STYLE, WS_OVERLAPPEDWINDOW as isize);
            let w = (rect.right - rect.left - 80).clamp(320, 960);
            let h = (w * 9 / 16 + 40).min(rect.bottom - rect.top - 80).max(240);
            SetWindowPos(
                self.output,
                HWND_NOTOPMOST,
                rect.left + 40,
                rect.top + 40,
                w,
                h,
                SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
    }
    unsafe fn toggle_fullscreen(&mut self) {
        if self.output.is_null() || IsWindowVisible(self.output) == 0 {
            self.show_output();
        }
        self.fullscreen = !self.fullscreen;
        self.position_output();
    }
    unsafe fn import(&mut self, bible: bool) {
        if !self.commit_editor() {
            return;
        }
        let mut filename = vec![0u16; 32768];
        let filter = if bible {
            wide("Ayat TSV UTF-8 (*.tsv)\0*.tsv\0Semua berkas\0*.*\0")
        } else {
            wide("Lirik UTF-8 (*.txt)\0*.txt\0Semua berkas\0*.*\0")
        };
        let mut dialog: OPENFILENAMEW = zeroed();
        dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
        dialog.hwndOwner = self.hwnd;
        dialog.lpstrFilter = filter.as_ptr();
        dialog.lpstrFile = filename.as_mut_ptr();
        dialog.nMaxFile = filename.len() as u32;
        dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
        if GetOpenFileNameW(&mut dialog) == 0 {
            return;
        }
        let end = filename
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(filename.len());
        let path = PathBuf::from(String::from_utf16_lossy(&filename[..end]));
        let result = if bible {
            self.library.import_verses_file(&path).map(|count| {
                format!("{count} ayat baru diimpor. Cari berdasarkan referensi atau isi ayat.")
            })
        } else {
            self.library.import_song_file(&path).map(|id| {
                self.select(id);
                "Lirik diimpor. Baris kosong memisahkan bagian / slide.".to_owned()
            })
        };
        match result {
            Ok(value) => {
                self.refresh_library();
                if self.persist() {
                    self.status(&value);
                }
            }
            Err(e) => {
                message(self.hwnd, "Impor gagal", &e, MB_OK | MB_ICONERROR);
            }
        }
    }
    unsafe fn command(&mut self, id: u16, event: u16) {
        match id {
            SEARCH if event == EN_CHANGE as u16 => self.refresh_library(),
            FILTER if event == CBN_SELCHANGE as u16 => self.refresh_library(),
            TITLE | BODY if event == EN_CHANGE as u16 => {
                self.dirty = true;
                SetTimer(self.hwnd, SAVE_TIMER, 1500, None);
            }
            LIBRARY if event == LBN_SELCHANGE as u16 => {
                let index = SendMessageW(self.c(LIBRARY), LB_GETCURSEL, 0, 0);
                let id = self.filtered.get(index as usize).copied();
                if self.commit_editor()
                    && let Some(id) = id
                {
                    self.select(id);
                }
            }
            LIBRARY if event == LBN_DBLCLK as u16 => self.command(ADD, 0),
            SERVICE if event == LBN_SELCHANGE as u16 || event == LBN_DBLCLK as u16 => {
                let index = SendMessageW(self.c(SERVICE), LB_GETCURSEL, 0, 0);
                let id = self.library.service.get(index as usize).copied();
                if self.commit_editor()
                    && let Some(id) = id
                {
                    self.select(id);
                    if event == LBN_DBLCLK as u16 {
                        self.go_live();
                    }
                }
            }
            SLIDES if event == LBN_SELCHANGE as u16 || event == LBN_DBLCLK as u16 => {
                let index = SendMessageW(self.c(SLIDES), LB_GETCURSEL, 0, 0);
                if index >= 0 {
                    self.page = index as usize;
                    InvalidateRect(self.c(PREVIEW), null(), 0);
                }
                if event == LBN_DBLCLK as u16 {
                    self.go_live();
                }
            }
            NEW => {
                if !self.commit_editor() {
                    return;
                }
                let kind = self.kind_filter().unwrap_or(Kind::Song);
                let id = self.library.add(
                    kind,
                    if kind == Kind::Verse {
                        "Ayat baru"
                    } else {
                        "Lagu baru"
                    }
                    .into(),
                    String::new(),
                );
                set_text(self.c(SEARCH), "");
                self.select(id);
                self.refresh_library();
                self.persist();
                SetFocus(self.c(TITLE));
                SendMessageW(self.c(TITLE), EM_SETSEL, 0, -1);
            }
            SAVE => {
                if self.commit_editor() {
                    self.persist();
                    self.status(
                        "Materi dan susunan tersimpan lokal. Baris kosong memisahkan slide.",
                    );
                }
            }
            DELETE => {
                if let Some(id) = self.selected
                    && message(
                        self.hwnd,
                        "Hapus materi",
                        "Hapus materi ini dari pustaka dan susunan ibadah?",
                        MB_YESNO | MB_ICONQUESTION,
                    ) == IDYES
                {
                    self.library.remove(id);
                    self.selected = None;
                    self.dirty = false;
                    set_text(self.c(TITLE), "");
                    set_text(self.c(BODY), "");
                    self.refresh_library();
                    self.refresh_service();
                    self.refresh_slides();
                    self.persist();
                }
            }
            ADD => {
                if !self.commit_editor() {
                    return;
                }
                if let Some(id) = self.selected {
                    self.library.service.push(id);
                    self.refresh_service();
                    self.persist();
                    SendMessageW(
                        self.c(SERVICE),
                        LB_SETCURSEL,
                        self.library.service.len() - 1,
                        0,
                    );
                }
            }
            UP | DOWN | REMOVE => {
                let index = SendMessageW(self.c(SERVICE), LB_GETCURSEL, 0, 0);
                if index < 0 {
                    return;
                }
                let index = index as usize;
                let target = if id == UP {
                    index.saturating_sub(1)
                } else {
                    (index + 1).min(self.library.service.len().saturating_sub(1))
                };
                if id == REMOVE {
                    self.library.service.remove(index);
                    self.live_service = self.live_service.and_then(|live| {
                        if live == index {
                            None
                        } else {
                            Some(if live > index { live - 1 } else { live })
                        }
                    });
                } else {
                    self.library.service.swap(index, target);
                    self.live_service = self.live_service.map(|live| {
                        if live == index {
                            target
                        } else if live == target {
                            index
                        } else {
                            live
                        }
                    });
                }
                self.refresh_service();
                if !self.library.service.is_empty() {
                    SendMessageW(
                        self.c(SERVICE),
                        LB_SETCURSEL,
                        target.min(self.library.service.len() - 1),
                        0,
                    );
                }
                self.persist();
            }
            LIVE => self.go_live(),
            BLACKOUT => {
                self.blackout = !self.blackout;
                self.refresh_live();
            }
            PROJECTOR => {
                if !self.output.is_null() && IsWindowVisible(self.output) != 0 {
                    ShowWindow(self.output, SW_HIDE);
                    set_text(self.c(PROJECTOR), "Buka proyektor");
                } else {
                    self.show_output();
                }
            }
            PREVIOUS => self.navigate(false),
            NEXT => self.navigate(true),
            IMPORT_TXT => self.import(false),
            IMPORT_BIBLE => self.import(true),
            MONITOR if event == CBN_SELCHANGE as u16 => {
                self.library.monitor =
                    SendMessageW(self.c(MONITOR), CB_GETCURSEL, 0, 0).max(0) as usize;
                if !self.output.is_null() {
                    self.position_output();
                }
                self.persist();
            }
            BACKGROUND if event == CBN_SELCHANGE as u16 => {
                self.library.background =
                    SendMessageW(self.c(BACKGROUND), CB_GETCURSEL, 0, 0).max(0) as usize;
                InvalidateRect(self.c(PREVIEW), null(), 0);
                self.refresh_live();
                self.persist();
            }
            _ => {}
        }
    }
    unsafe fn smoke_tick(&mut self) {
        let Some(step) = self.smoke_step else {
            return;
        };
        match step {
            0 => {
                self.select(1);
                self.go_live();
                self.smoke_checks.push((
                    "first_slide".into(),
                    self.live
                        .as_ref()
                        .is_some_and(|s| s.item == 1 && s.page == 0),
                ));
            }
            1 => {
                self.navigate(true);
                self.smoke_checks.push((
                    "next_slide".into(),
                    self.live
                        .as_ref()
                        .is_some_and(|s| s.item == 1 && s.page == 1),
                ));
            }
            2 => {
                self.navigate(true);
                self.smoke_checks.push((
                    "next_service_item".into(),
                    self.live
                        .as_ref()
                        .is_some_and(|s| s.item == 2 && s.page == 0),
                ));
            }
            3 => {
                self.navigate(false);
                self.smoke_checks.push((
                    "previous_service_item".into(),
                    self.live
                        .as_ref()
                        .is_some_and(|s| s.item == 1 && s.page == 1),
                ));
            }
            4 => {
                self.blackout = true;
                self.refresh_live();
                self.smoke_checks.push(("blackout".into(), self.blackout));
            }
            5 => {
                self.blackout = false;
                self.refresh_live();
                if !self.fullscreen {
                    self.toggle_fullscreen();
                }
                self.smoke_checks.push((
                    "fullscreen".into(),
                    self.fullscreen
                        && (GetWindowLongPtrW(self.output, GWL_STYLE) as u32 & WS_POPUP) != 0,
                ));
            }
            6 => {
                if self.fullscreen {
                    self.toggle_fullscreen();
                }
                self.smoke_checks.push((
                    "windowed".into(),
                    !self.fullscreen
                        && (GetWindowLongPtrW(self.output, GWL_STYLE) as u32 & WS_CAPTION) != 0,
                ));
                set_text(self.c(SEARCH), "BERSYUKUR");
                self.refresh_library();
                self.smoke_checks
                    .push(("search".into(), self.filtered == vec![1]));
                set_text(self.c(SEARCH), "");
                self.refresh_library();
                set_text(self.c(TITLE), "Judul uji ✓");
                self.dirty = true;
                let saved = self.commit_editor();
                let title = Library::load(&self.path)
                    .ok()
                    .and_then(|l| l.item(1).map(|i| i.title.clone()));
                self.smoke_checks.push((
                    "unicode_edit_saved".into(),
                    saved && title.as_deref() == Some("Judul uji ✓"),
                ));
            }
            7 => {
                let report = serde_json::json!({"ok":self.smoke_checks.iter().all(|(_,ok)|*ok),"checks":self.smoke_checks,"version":"0.1.0","output_window":!self.output.is_null(),"monitors":self.monitors.len(),"data_saved":self.path.exists()});
                let _ = std::fs::write(
                    &self.smoke_report,
                    serde_json::to_vec_pretty(&report).unwrap(),
                );
                KillTimer(self.hwnd, SMOKE_TIMER);
                PostMessageW(self.hwnd, WM_CLOSE, 0, 0);
            }
            _ => {}
        }
        self.smoke_step = Some(step + 1);
    }
}

unsafe fn fill(hdc: HDC, rect: &RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, rect, brush);
    DeleteObject(brush);
}
unsafe fn label(
    hdc: HDC,
    value: &str,
    rect: &mut RECT,
    size: i32,
    color: u32,
    flags: u32,
    bold: bool,
) {
    let font = CreateFontW(
        -size,
        0,
        0,
        0,
        if bold { FW_SEMIBOLD } else { FW_NORMAL } as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide("Segoe UI").as_ptr(),
    );
    let old = SelectObject(hdc, font);
    SetTextColor(hdc, color);
    SetBkMode(hdc, TRANSPARENT as i32);
    let value = wide(value);
    DrawTextW(hdc, value.as_ptr(), -1, rect, flags | DT_NOPREFIX);
    SelectObject(hdc, old);
    DeleteObject(font);
}

unsafe fn paint_main(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rect: RECT = zeroed();
    GetClientRect(hwnd, &mut rect);
    fill(hdc, &rect, SURFACE);
    let scale = GetDpiForWindow(hwnd).max(96) as f64 / 96.;
    let px = |n: i32| (n as f64 * scale) as i32;
    let mut title = RECT {
        left: px(16),
        top: px(10),
        right: rect.right - px(16),
        bottom: px(40),
    };
    label(
        hdc,
        "LENTERA",
        &mut title,
        px(24),
        ACCENT,
        DT_LEFT | DT_SINGLELINE,
        true,
    );
    let mut subtitle = RECT {
        left: px(170),
        top: px(19),
        right: rect.right - px(16),
        bottom: px(45),
    };
    label(
        hdc,
        "Presentasi ibadah yang ringan. Fokus pada firman dan pujian.",
        &mut subtitle,
        px(12),
        rgb(90, 107, 122),
        DT_LEFT | DT_SINGLELINE,
        false,
    );
    EndPaint(hwnd, &ps);
}

unsafe fn paint_slide(hwnd: HWND) {
    let snapshot = APP.with(|cell| {
        cell.try_borrow().ok().and_then(|a| {
            a.as_ref().map(|app| {
                let preview = hwnd == app.c(PREVIEW);
                let slide = if preview {
                    app.selected.and_then(|id| app.library.slide(id, app.page))
                } else {
                    app.live.clone()
                };
                (
                    slide,
                    !preview && app.blackout,
                    app.library.background,
                    preview,
                )
            })
        })
    });
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rect: RECT = zeroed();
    GetClientRect(hwnd, &mut rect);
    let (slide, blackout, theme, preview) = snapshot.unwrap_or((None, false, 0, false));
    let background = if blackout {
        0
    } else {
        match theme {
            1 => rgb(11, 35, 56),
            2 => rgb(252, 249, 241),
            _ => rgb(15, 22, 31),
        }
    };
    fill(hdc, &rect, background);
    if !blackout {
        let foreground = if theme == 2 { INK } else { rgb(250, 251, 253) };
        if let Some(slide) = slide {
            let height = rect.bottom.max(1);
            let pad = (rect.right / 22).max(12);
            let mut area = RECT {
                left: pad,
                top: pad,
                right: rect.right - pad,
                bottom: height - pad - height / 10,
            };
            let content = wide(&slide.text);
            let mut size = (height * 12 / 100).clamp(10, 110);
            let mut font: HFONT;
            let mut old: HGDIOBJ;
            loop {
                font = CreateFontW(
                    -size,
                    0,
                    0,
                    0,
                    FW_SEMIBOLD as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    CLEARTYPE_QUALITY as u32,
                    DEFAULT_PITCH as u32,
                    wide("Segoe UI").as_ptr(),
                );
                old = SelectObject(hdc, font);
                let mut measured = area;
                DrawTextW(
                    hdc,
                    content.as_ptr(),
                    -1,
                    &mut measured,
                    DT_CENTER | DT_WORDBREAK | DT_CALCRECT | DT_NOPREFIX,
                );
                if (measured.bottom - measured.top <= area.bottom - area.top
                    && measured.right - measured.left <= area.right - area.left)
                    || size <= 8
                {
                    area.top +=
                        (area.bottom - area.top - (measured.bottom - measured.top)).max(0) / 2;
                    break;
                }
                SelectObject(hdc, old);
                DeleteObject(font);
                size -= 1;
            }
            SetTextColor(hdc, foreground);
            SetBkMode(hdc, TRANSPARENT as i32);
            DrawTextW(
                hdc,
                content.as_ptr(),
                -1,
                &mut area,
                DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
            );
            SelectObject(hdc, old);
            DeleteObject(font);
            let mut footer = RECT {
                left: pad,
                top: height - height / 10,
                right: rect.right - pad,
                bottom: height - pad / 3,
            };
            label(
                hdc,
                &format!("{}   ·   {}/{}", slide.title, slide.page + 1, slide.total),
                &mut footer,
                (height / 24).clamp(9, 30),
                foreground,
                DT_CENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                false,
            );
        } else if preview {
            let size = (rect.bottom / 18).clamp(10, 24);
            label(
                hdc,
                "Pilih materi untuk pratinjau",
                &mut rect,
                size,
                foreground,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                false,
            );
        }
    }
    EndPaint(hwnd, &ps);
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_main(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            with_app(|a| a.layout());
            0
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lp as *mut MINMAXINFO);
            info.ptMinTrackSize = POINT { x: 900, y: 660 };
            0
        }
        WM_COMMAND => {
            with_app(|a| a.command((wp & 0xffff) as u16, ((wp >> 16) & 0xffff) as u16));
            0
        }
        WM_TIMER => {
            with_app(|a| {
                if wp == SAVE_TIMER {
                    KillTimer(hwnd, SAVE_TIMER);
                    a.commit_editor();
                } else if wp == SMOKE_TIMER {
                    a.smoke_tick();
                }
            });
            0
        }
        WM_CTLCOLORSTATIC => {
            SetBkColor(wp as HDC, SURFACE);
            SetTextColor(wp as HDC, INK);
            SetDCBrushColor(wp as HDC, SURFACE);
            GetStockObject(DC_BRUSH) as LRESULT
        }
        WM_DRAWITEM => {
            let item = &*(lp as *const DRAWITEMSTRUCT);
            if item.CtlID == LIVE as u32 {
                fill(
                    item.hDC,
                    &item.rcItem,
                    if item.itemState & ODS_SELECTED != 0 {
                        rgb(0, 85, 90)
                    } else {
                        ACCENT
                    },
                );
                let mut rect = item.rcItem;
                label(
                    item.hDC,
                    "Tayangkan [F5]",
                    &mut rect,
                    16,
                    rgb(255, 255, 255),
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    true,
                );
                if item.itemState & ODS_FOCUS != 0 {
                    DrawFocusRect(item.hDC, &item.rcItem);
                }
                return 1;
            }
            0
        }
        WM_CLOSE => {
            let mut close = true;
            with_app(|a| {
                close = a.commit_editor() && a.persist();
            });
            if close {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe extern "system" fn slide_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_slide(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe extern "system" fn output_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_slide(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            with_app(|a| set_text(a.c(PROJECTOR), "Buka proyektor"));
            0
        }
        WM_LBUTTONDBLCLK => {
            with_app(|a| a.toggle_fullscreen());
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    let mut directory = None;
    let mut smoke = false;
    for index in 1..args.len() {
        if args[index] == "--data-dir" {
            directory = args.get(index + 1).map(PathBuf::from);
        }
        if args[index] == "--ui-smoke-test" {
            smoke = true;
        }
    }
    if smoke && directory.is_none() {
        return Err("Pengujian UI wajib memakai --data-dir terpisah.".into());
    }
    let directory = directory.unwrap_or_else(default_data_dir);
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
    let instance = GetModuleHandleW(null());
    for (name, proc, style) in [
        (
            "LenteraMain",
            Some(main_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            CS_HREDRAW | CS_VREDRAW,
        ),
        (
            "LenteraSlide",
            Some(slide_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            CS_HREDRAW | CS_VREDRAW,
        ),
        (
            "LenteraOutput",
            Some(output_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
        ),
    ] {
        let mut class: WNDCLASSW = zeroed();
        let name = wide(name);
        class.style = style;
        class.lpfnWndProc = proc;
        class.hInstance = instance;
        class.lpszClassName = name.as_ptr();
        class.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        class.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        if RegisterClassW(&class) == 0 {
            return Err(format!(
                "Tidak bisa mendaftarkan jendela: {}",
                GetLastError()
            ));
        }
    }
    let width = GetSystemMetrics(SM_CXSCREEN).min(1220);
    let height = GetSystemMetrics(SM_CYSCREEN).saturating_sub(70).min(840);
    let hwnd = CreateWindowExW(
        0,
        wide("LenteraMain").as_ptr(),
        wide("Lentera · Presentasi Ibadah Ringan").as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        width,
        height,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        return Err("Jendela utama tidak bisa dibuat.".into());
    }
    let app = App::new(hwnd, directory.join(LIBRARY_FILE), smoke)?;
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    with_app(|a| {
        a.layout();
        a.persist();
    });
    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);
    if smoke {
        SetTimer(hwnd, SMOKE_TIMER, 400, None);
    }
    let mut msg: MSG = zeroed();
    loop {
        let result = GetMessageW(&mut msg, null_mut(), 0, 0);
        if result == 0 {
            break;
        }
        if result < 0 {
            return Err("Pembacaan pesan Windows gagal.".into());
        }
        if msg.message == WM_KEYDOWN {
            let key = msg.wParam as u16;
            let control = GetKeyState(VK_CONTROL as i32) < 0;
            let mut handled = true;
            with_app(|app| {
                let editing = GetFocus() == app.c(BODY)
                    || GetFocus() == app.c(TITLE)
                    || GetFocus() == app.c(SEARCH);
                match key {
                    VK_F5 => app.go_live(),
                    VK_F6 => {
                        app.blackout = !app.blackout;
                        app.refresh_live();
                    }
                    VK_F11 => app.toggle_fullscreen(),
                    VK_NEXT if !editing => app.navigate(true),
                    VK_PRIOR if !editing => app.navigate(false),
                    VK_RIGHT | VK_SPACE if !editing => app.navigate(true),
                    VK_LEFT if !editing => app.navigate(false),
                    VK_ESCAPE => {
                        if !app.output.is_null() {
                            if app.fullscreen {
                                app.fullscreen = false;
                                app.position_output();
                            } else {
                                ShowWindow(app.output, SW_HIDE);
                                set_text(app.c(PROJECTOR), "Buka proyektor");
                            }
                        }
                    }
                    0x53 if control => app.command(SAVE, 0),
                    0x4e if control => app.command(NEW, 0),
                    VK_RETURN if control => app.go_live(),
                    _ => handled = false,
                }
            });
            if handled {
                continue;
            }
        }
        if IsDialogMessageW(hwnd, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().take() {
            if !app.output.is_null() {
                DestroyWindow(app.output);
            }
            DeleteObject(app.font);
        }
    });
    Ok(())
}
fn main() {
    unsafe {
        if let Err(error) = run() {
            message(
                null_mut(),
                "Lentera tidak bisa dibuka",
                &error,
                MB_OK | MB_ICONERROR,
            );
            std::process::exit(1);
        }
    }
}
