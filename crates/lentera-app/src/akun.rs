//! Masuk ke akun Lentera dan sinkron profil dengan server (protokolnya di
//! `docs/SINKRON.md`).
//!
//! Alamat server diisi saat build rilis (variabel `LENTERA_SERVER`) atau
//! dengan `--server <alamat>`. Tanpa alamat server, Lentera berjalan seperti
//! sebelumnya: semua data hanya ada di komputer ini dan layar masuk tidak
//! pernah tampil.
//!
//! Permintaan HTTP berjalan di thread lain. Basis data hanya diubah di thread
//! tampilan, di antara permintaan (lihat [`lentera_core::sync::Engine`]).

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use lentera_core::sync::{
    ApiError, Engine, HttpApi, Reply, SignedIn, Stop, Verified, device_name, perform, run,
};
use lentera_core::{LIBRARY_FILE, profiles};
use rfd::{MessageButtons, MessageDialogResult, MessageLevel};
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode};

use crate::{App, Launch, LoginWindow, VERSION, dialog, open_app, window_icons};

/// Alamat server bawaan rilis. Kosong di build pengembangan.
const BUILT_IN_SERVER: Option<&str> = option_env!("LENTERA_SERVER");
/// Jeda sinkron otomatis.
const SYNC_EVERY: Duration = Duration::from_secs(30);
/// "Menyinkron…" baru tampil bila putaran lebih lama dari ini, supaya bilah
/// atas tidak berkedip setiap 30 detik.
const BUSY_AFTER: Duration = Duration::from_millis(800);
/// Selisih jam komputer dan server yang diperingatkan. Perubahan terbaru
/// ditentukan dari jam komputer yang mengubahnya.
const CLOCK_SKEW_MS: i64 = 5 * 60 * 1000;
/// Nama layanan token di Windows Credential Manager / Keychain.
const KEYRING_SERVICE: &str = "Lentera";

/// Alamat server yang dipakai, atau `None` bila Lentera tidak memakai akun.
/// Uji alur otomatis hanya memakai server bila diberi `--server`.
pub fn server(arg: Option<&str>, smoke: bool) -> Option<String> {
    let url = match arg {
        Some(url) => url,
        None if smoke => return None,
        None => BUILT_IN_SERVER?,
    };
    let url = url.trim().trim_end_matches('/');
    (!url.is_empty()).then(|| url.to_owned())
}

// ----- Token sesi -----

/// Token disimpan per server dan per folder data, supaya folder data uji
/// (`--data-dir`) tidak memakai sesi folder data utama.
fn token_entry(server: &str, data_dir: &Path) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("{server}|{}", data_dir.display()))
        .map_err(|e| format!("Penyimpanan kredensial Windows tidak bisa dipakai.\n{e}"))
}

fn load_token(server: &str, data_dir: &Path) -> Option<String> {
    token_entry(server, data_dir).ok()?.get_password().ok()
}

fn save_token(server: &str, data_dir: &Path, token: &str) -> Result<(), String> {
    token_entry(server, data_dir)?
        .set_password(token)
        .map_err(|e| format!("Sesi akun tidak bisa disimpan di komputer ini.\n{e}"))
}

fn forget_token(server: &str, data_dir: &Path) {
    if let Ok(entry) = token_entry(server, data_dir) {
        let _ = entry.delete_credential();
    }
}

/// Akun yang sudah masuk di folder data ini, bila sesinya masih tersimpan.
pub fn saved_account(server: &str, data_dir: &Path) -> Option<Account> {
    let signed = SignedIn::load(data_dir).ok().flatten()?;
    if signed.server != server {
        return None;
    }
    let token = load_token(server, data_dir)?;
    Some(Account::new(signed, token))
}

/// Folder data sudah berisi profil atau `library.json` lama.
fn has_local_data(data_dir: &Path) -> bool {
    data_dir.join(LIBRARY_FILE).exists() || profiles(data_dir).map_or(true, |p| !p.is_empty())
}

// ----- Pekerjaan di thread lain -----

thread_local! {
    /// Lanjutan pekerjaan latar yang menunggu hasil, menurut nomornya.
    static WAITING: RefCell<HashMap<u64, Box<dyn FnOnce()>>> = RefCell::default();
}
static NEXT_JOB: AtomicU64 = AtomicU64::new(0);

/// Menjalankan `job` di thread lain, lalu `done` dengan hasilnya di thread
/// tampilan. `done` tidak dipanggil bila Lentera sudah ditutup.
fn in_background<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) + 'static,
) {
    let id = NEXT_JOB.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    WAITING.with(|waiting| {
        waiting.borrow_mut().insert(
            id,
            Box::new(move || {
                if let Ok(value) = rx.try_recv() {
                    done(value);
                }
            }),
        );
    });
    std::thread::spawn(move || {
        let _ = tx.send(job());
        let _ = slint::invoke_from_event_loop(move || {
            let next = WAITING.with(|waiting| waiting.borrow_mut().remove(&id));
            if let Some(next) = next {
                next();
            }
        });
    });
}

// ----- Akun -----

/// Akun yang masuk dan putaran sinkronnya.
pub struct Account {
    signed: SignedIn,
    api: Arc<HttpApi>,
    /// Putaran yang sedang berjalan.
    engine: Option<Engine>,
    /// Sinkron diminta lagi saat putaran berjalan.
    again: bool,
    /// Server minta menunggu sampai saat ini.
    wait_until: Option<Instant>,
    /// Sesi berakhir; sinkron berhenti sampai masuk lagi.
    login_needed: bool,
}

impl Account {
    fn new(signed: SignedIn, token: String) -> Self {
        Self {
            api: Arc::new(HttpApi::new(&signed.server, Some(token))),
            signed,
            engine: None,
            again: false,
            wait_until: None,
            login_needed: false,
        }
    }
}

/// Keterangan bila jam komputer ini jauh dari jam server.
fn clock_note(server_ms: i64) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    let diff = now - server_ms;
    if diff.abs() <= CLOCK_SKEW_MS {
        return None;
    }
    let minutes = diff.abs() / 60_000;
    let side = if diff > 0 {
        "lebih cepat"
    } else {
        "lebih lambat"
    };
    Some(format!(
        "Jam komputer ini {side} {minutes} menit dari jam server. Perbaiki tanggal dan jam \
         Windows supaya perubahan terbaru tidak tertimpa perubahan lama."
    ))
}

impl App {
    /// Lentera memakai server, tetapi dibuka tanpa masuk.
    pub(crate) fn signed_out(&self, server: String) {
        *self.server.borrow_mut() = Some(server);
        self.ui.set_account_email(Default::default());
        self.show_sync(
            3,
            "Belum masuk",
            "Lentera dibuka tanpa masuk karena server tidak terjangkau. Masuk untuk \
             menyinkronkan profil dengan akun Anda.",
            true,
        );
    }

    /// Mulai sinkron otomatis untuk akun ini.
    pub(crate) fn start_sync(&self, account: Account) {
        *self.server.borrow_mut() = Some(account.signed.server.clone());
        self.ui
            .set_account_email(account.signed.email.as_str().into());
        self.show_sync(1, "Menyinkron…", "Sinkron pertama sedang berjalan.", false);
        *self.account.borrow_mut() = Some(account);
        let weak = self.me.clone();
        self.sync_timer
            .start(TimerMode::Repeated, SYNC_EVERY, move || {
                if let Some(app) = weak.upgrade() {
                    app.sync_now();
                }
            });
        self.sync_now();
    }

    fn show_sync(&self, state: i32, label: &str, detail: &str, login: bool) {
        self.ui.set_sync_state(state);
        self.ui.set_sync_label(label.into());
        self.ui.set_sync_detail(detail.into());
        self.ui.set_sync_login(login);
    }

    /// Memulai satu putaran sinkron, atau mengulanginya setelah putaran yang
    /// sedang berjalan selesai.
    pub(crate) fn sync_now(&self) {
        let (account, removed) = {
            let mut slot = self.account.borrow_mut();
            let Some(acc) = slot.as_mut() else {
                return;
            };
            if acc.login_needed {
                return;
            }
            if acc.engine.is_some() {
                acc.again = true;
                return;
            }
            if acc.wait_until.is_some_and(|until| Instant::now() < until) {
                return;
            }
            acc.wait_until = None;
            (acc.signed.account.clone(), acc.signed.removed.clone())
        };
        // Profil yang sedang dibuka disinkron lebih dulu.
        let first = self.state.borrow().store.id().to_owned();
        let engine = Engine::new(self.data_dir.clone(), account, Some(first), removed);
        if let Some(acc) = self.account.borrow_mut().as_mut() {
            acc.engine = Some(engine);
            acc.again = false;
        }
        let weak = self.me.clone();
        self.sync_busy_timer
            .start(TimerMode::SingleShot, BUSY_AFTER, move || {
                if let Some(app) = weak.upgrade() {
                    app.show_sync(1, "Menyinkron…", "Mengirim dan mengambil perubahan.", false);
                }
            });
        self.sync_step();
    }

    /// Menulis perubahan di memori ke basis data, supaya ikut terkirim dan
    /// tidak tertimpa jawaban server.
    fn flush(&self) -> Result<(), String> {
        let st = self.state.borrow();
        st.store.save(&st.library)
    }

    fn sync_step(&self) {
        if let Err(e) = self.flush() {
            return self.sync_aborted(&e);
        }
        let next = {
            let mut slot = self.account.borrow_mut();
            let Some(acc) = slot.as_mut() else {
                return;
            };
            let api = acc.api.clone();
            let Some(engine) = acc.engine.as_mut() else {
                return;
            };
            engine.next_call().map(|call| (call, api))
        };
        let Some((call, api)) = next else {
            return self.sync_finished();
        };
        let weak = self.me.clone();
        in_background(
            move || perform(api.as_ref(), &call),
            move |reply| {
                if let Some(app) = weak.upgrade() {
                    app.sync_reply(reply);
                }
            },
        );
    }

    fn sync_reply(&self, reply: Result<Reply, ApiError>) {
        if let Err(e) = self.flush() {
            return self.sync_aborted(&e);
        }
        let effect = {
            let mut slot = self.account.borrow_mut();
            // Keluar dari akun saat permintaan berjalan: jawabannya dibuang.
            let Some(engine) = slot.as_mut().and_then(|acc| acc.engine.as_mut()) else {
                return;
            };
            engine.handle(reply)
        };
        let current = self.state.borrow().store.id().to_owned();
        if effect.changed.as_deref() == Some(current.as_str()) {
            self.reload_profile();
        }
        if (effect.profiles || effect.changed.is_some()) && self.ui.get_picking_profile() {
            self.refresh_profiles();
        }
        self.sync_step();
    }

    fn sync_aborted(&self, error: &str) {
        self.sync_busy_timer.stop();
        if let Some(acc) = self.account.borrow_mut().as_mut() {
            acc.engine = None;
            acc.again = false;
        }
        self.show_sync(
            3,
            "Sinkron gagal",
            &format!("Perubahan belum tersimpan di komputer ini, jadi belum dikirim.\n{error}"),
            false,
        );
    }

    fn sync_finished(&self) {
        self.sync_busy_timer.stop();
        let (report, again) = {
            let mut slot = self.account.borrow_mut();
            let Some(acc) = slot.as_mut() else {
                return;
            };
            let Some(engine) = acc.engine.take() else {
                return;
            };
            let report = engine.report().clone();
            match report.stop {
                Some(Stop::LoginNeeded) => acc.login_needed = true,
                Some(Stop::Wait(seconds)) => {
                    acc.wait_until = Some(Instant::now() + Duration::from_secs(seconds))
                }
                _ => {}
            }
            (report, std::mem::take(&mut acc.again))
        };
        let mut problems = report.problems.clone();
        problems.extend(report.server_time.and_then(clock_note));
        match &report.stop {
            None if problems.is_empty() => self.show_sync(
                0,
                "Tersinkron",
                "Profil di komputer ini sama dengan di akun. Sinkron berjalan otomatis setiap \
                 30 detik saat online.",
                false,
            ),
            None => self.show_sync(3, "Perlu perhatian", &problems.join("\n\n"), false),
            Some(Stop::Offline(_)) => self.show_sync(
                2,
                "Offline",
                "Server Lentera tidak terjangkau. Perubahan tetap tersimpan di komputer ini dan \
                 dikirim saat online lagi.",
                false,
            ),
            Some(Stop::LoginNeeded) => self.show_sync(
                3,
                "Masuk lagi",
                "Sesi akun berakhir, misalnya karena keluar dari komputer lain. Masuk lagi untuk \
                 melanjutkan sinkron. Data di komputer ini tetap bisa dipakai.",
                true,
            ),
            Some(Stop::Wait(seconds)) => self.show_sync(
                2,
                "Menunggu",
                &format!("Server sedang sibuk. Sinkron dicoba lagi dalam {seconds} detik."),
                false,
            ),
            Some(Stop::Failed(message)) => self.show_sync(3, "Sinkron gagal", message, false),
        }
        if again && report.stop.is_none() {
            self.sync_now();
        }
    }

    /// Membaca ulang profil yang terbuka setelah sinkron mengubahnya. Slide
    /// yang sedang tayang tidak berubah (salinan).
    fn reload_profile(&self) {
        // Isi editor yang belum tersimpan lebih baru dari perubahan server.
        if self.state.borrow().dirty && !self.commit_editor() {
            return;
        }
        let result = self.state.borrow_mut().store.reload();
        let library = match result {
            Ok(library) => library,
            Err(e) => {
                self.status(
                    &format!("Perubahan dari akun belum bisa dimuat.\n{e}"),
                    false,
                );
                return;
            }
        };
        let (background, selected_changed, selection_gone) = {
            let mut st = self.state.borrow_mut();
            let old = std::mem::replace(&mut st.library, library);
            let selected = st.selected;
            let before = selected.and_then(|id| old.item(id));
            let after = selected.and_then(|id| st.library.item(id));
            let selected_changed = before != after;
            let selection_gone = selected.is_some() && after.is_none();
            // Monitor proyektor dan materi sementara milik komputer ini.
            st.library.monitor = old.monitor;
            st.library.scratch = old.scratch;
            if let Some(id) = selected
                && !selection_gone
                && st
                    .selected_service
                    .is_some_and(|i| st.library.service.get(i) != Some(&id))
            {
                st.selected_service = st.library.service.iter().position(|s| *s == id);
            }
            if st
                .live_service
                .is_some_and(|i| i >= st.library.service.len())
            {
                st.live_service = None;
            }
            (
                st.library.background as i32,
                selected_changed,
                selection_gone,
            )
        };
        self.ui.set_bg_index(background);
        self.output.set_bg_index(background);
        self.apply_background();
        self.refresh_library();
        self.refresh_service();
        self.refresh_live();
        if selection_gone {
            self.select_first();
        } else if selected_changed {
            self.load_editor();
            self.refresh_current();
        } else {
            self.refresh_current();
        }
        self.refresh_profile_label();
    }

    /// Profil dihapus di komputer ini: jangan diunduh lagi, dan hapus juga
    /// di akun bila akun ini pemiliknya.
    pub(crate) fn sync_profile_removed(&self, id: &str) {
        let saved = {
            let mut slot = self.account.borrow_mut();
            let Some(acc) = slot.as_mut() else {
                return;
            };
            acc.signed.remember_removed(id);
            acc.signed.save(&self.data_dir)
        };
        if let Err(e) = saved {
            self.status(
                &format!("Penghapusan profil belum tercatat untuk akun.\n{e}"),
                false,
            );
        }
        self.sync_now();
    }

    /// Layar masuk dari menu akun: sesi berakhir, atau Lentera dibuka tanpa
    /// masuk.
    pub(crate) fn sign_in_again(&self) {
        if let Some(login) = self.relogin.borrow().as_ref() {
            let _ = login.window.show();
            return;
        }
        let Some(server) = self.server.borrow().clone() else {
            return;
        };
        let expected = self.account.borrow().as_ref().map(|a| a.signed.clone());
        let weak = self.me.clone();
        let login = Login::new(
            server,
            self.data_dir.clone(),
            expected,
            true,
            false,
            move |login, outcome| {
                let _ = login.window.hide();
                let Some(app) = weak.upgrade() else {
                    return;
                };
                // Jendela dilepas setelah callback-nya selesai.
                let login = app.relogin.borrow_mut().take();
                Timer::single_shot(Duration::ZERO, move || drop(login));
                if let Outcome::SignedIn(account) = outcome {
                    app.start_sync(*account);
                    app.status("Masuk ke akun berhasil. Sinkron dilanjutkan.", true);
                }
            },
        );
        match login {
            Ok(login) => *self.relogin.borrow_mut() = Some(login),
            Err(e) => {
                dialog(
                    Some(self.ui.window()),
                    MessageLevel::Error,
                    "Layar masuk tidak bisa dibuka",
                    &e,
                    MessageButtons::Ok,
                );
            }
        }
    }

    /// Keluar dari akun: sesi di komputer ini dihapus, lalu Lentera ditutup.
    pub(crate) fn sign_out(&self) {
        let Some(email) = self
            .account
            .borrow()
            .as_ref()
            .map(|a| a.signed.email.clone())
        else {
            return;
        };
        let answer = dialog(
            Some(self.ui.window()),
            MessageLevel::Warning,
            "Keluar dari akun",
            &format!(
                "Keluar dari {email}?\n\n\
                 Lentera akan ditutup. Profil di komputer ini tetap ada, tetapi tidak disinkron \
                 lagi. Saat dibuka lagi, Lentera meminta masuk."
            ),
            MessageButtons::YesNo,
        );
        if answer != MessageDialogResult::Yes {
            return;
        }
        self.sync_timer.stop();
        let Some(account) = self.account.borrow_mut().take() else {
            return;
        };
        forget_token(&account.signed.server, &self.data_dir);
        if let Err(e) = SignedIn::forget(&self.data_dir) {
            dialog(
                Some(self.ui.window()),
                MessageLevel::Error,
                "Keluar dari akun",
                &e,
                MessageButtons::Ok,
            );
        }
        self.show_sync(1, "Keluar…", "Mengakhiri sesi di server.", false);
        // Sesi di server diakhiri bila terjangkau; tanpa internet, token di
        // komputer ini tetap sudah dihapus.
        let api = account.api.clone();
        let weak = self.me.clone();
        in_background(
            move || api.logout(),
            move |_| {
                if let Some(app) = weak.upgrade() {
                    app.close();
                }
            },
        );
    }
}

// ----- Layar masuk -----

pub enum Outcome {
    SignedIn(Box<Account>),
    /// Dibuka tanpa masuk (server tidak terjangkau).
    Skipped,
    Cancelled,
}

type Done = Box<dyn FnOnce(Rc<Login>, Outcome)>;

/// Jendela masuk dengan email dan kode 6 digit.
pub struct Login {
    window: LoginWindow,
    server: String,
    data_dir: PathBuf,
    /// Saat masuk lagi: akun yang dipakai di komputer ini.
    expected: Option<SignedIn>,
    /// Komputer ini sudah punya data, jadi boleh dibuka tanpa masuk bila
    /// server tidak terjangkau.
    has_data: bool,
    resend_timer: Timer,
    done: RefCell<Option<Done>>,
    busy: Cell<bool>,
    me: Weak<Login>,
}

impl Login {
    /// `in_app`: Lentera sudah terbuka, jadi ada tombol Batal dan tidak ada
    /// pilihan dibuka tanpa masuk.
    fn new(
        server: String,
        data_dir: PathBuf,
        expected: Option<SignedIn>,
        in_app: bool,
        has_data: bool,
        done: impl FnOnce(Rc<Login>, Outcome) + 'static,
    ) -> Result<Rc<Self>, String> {
        let window = LoginWindow::new().map_err(|e| e.to_string())?;
        window.set_version(VERSION.into());
        window.set_again(in_app);
        if let Some(signed) = &expected {
            window.set_email(signed.email.as_str().into());
        }
        let login = Rc::new_cyclic(|me| Self {
            window,
            server,
            data_dir,
            expected,
            has_data,
            resend_timer: Timer::default(),
            done: RefCell::new(Some(Box::new(done))),
            busy: Cell::new(false),
            me: me.clone(),
        });
        macro_rules! on {
            ($setter:ident, $method:ident) => {{
                let weak = Rc::downgrade(&login);
                login.window.$setter(move || {
                    if let Some(login) = weak.upgrade() {
                        login.$method();
                    }
                });
            }};
        }
        on!(on_send_code, send_code);
        on!(on_verify, verify);
        on!(on_change_email, change_email);
        on!(on_skip, skip);
        on!(on_cancel, cancel);
        let weak = Rc::downgrade(&login);
        login.window.window().on_close_requested(move || {
            if let Some(login) = weak.upgrade() {
                login.finish(Outcome::Cancelled);
            }
            CloseRequestResponse::HideWindow
        });
        login.window.show().map_err(|e| e.to_string())?;
        let weak = Rc::downgrade(&login);
        Timer::single_shot(Duration::from_millis(300), move || {
            if let Some(login) = weak.upgrade() {
                window_icons(login.window.window());
            }
        });
        Ok(login)
    }

    fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
        self.window.set_busy(busy);
    }

    fn email(&self) -> String {
        self.window.get_email().trim().to_owned()
    }

    fn send_code(&self) {
        if self.busy.get() {
            return;
        }
        if self.window.get_step() == 1 && self.window.get_resend_wait() > 0 {
            return;
        }
        let email = self.email();
        if email.is_empty() || !email.contains('@') || email.contains(char::is_whitespace) {
            self.window
                .set_error("Masukkan alamat email yang benar, misalnya nama@gereja.org.".into());
            return;
        }
        self.window.set_email(email.as_str().into());
        self.window.set_error(Default::default());
        self.set_busy(true);
        let api = HttpApi::new(&self.server, None);
        let weak = self.me.clone();
        in_background(
            move || api.send_code(&email),
            move |result| {
                let Some(login) = weak.upgrade() else {
                    return;
                };
                login.set_busy(false);
                match result {
                    Ok(sent) => {
                        login.window.set_step(1);
                        login.window.set_code(Default::default());
                        login.countdown(sent.seconds_until_resend.max(0));
                    }
                    Err(e) => login.failed(&e),
                }
            },
        );
    }

    /// Hitung mundur tombol Kirim ulang.
    fn countdown(&self, seconds: i64) {
        self.window.set_resend_wait(seconds.min(3600) as i32);
        let weak = self.me.clone();
        self.resend_timer
            .start(TimerMode::Repeated, Duration::from_secs(1), move || {
                let Some(login) = weak.upgrade() else {
                    return;
                };
                let left = login.window.get_resend_wait() - 1;
                login.window.set_resend_wait(left.max(0));
                if left <= 0 {
                    login.resend_timer.stop();
                }
            });
    }

    fn failed(&self, error: &ApiError) {
        self.window.set_error(error.message.as_str().into());
        if let Some(seconds) = error.retry_after {
            self.countdown(seconds as i64);
        }
        let unreachable = error.is_offline() || error.status >= 500;
        if unreachable && self.has_data && !self.window.get_again() {
            self.window.set_can_skip(true);
        }
    }

    fn verify(&self) {
        if self.busy.get() {
            return;
        }
        let code: String = self
            .window
            .get_code()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        if code.len() != 6 {
            self.window
                .set_error("Masukkan 6 digit kode dari email.".into());
            return;
        }
        self.window.set_error(Default::default());
        self.set_busy(true);
        let api = HttpApi::new(&self.server, None);
        let email = self.email();
        let weak = self.me.clone();
        in_background(
            move || api.verify(&email, &code, &device_name()),
            move |result| {
                let Some(login) = weak.upgrade() else {
                    return;
                };
                login.set_busy(false);
                match result {
                    Ok(verified) => login.verified(verified),
                    Err(e) => login.failed(&e),
                }
            },
        );
    }

    fn verified(&self, verified: Verified) {
        let Verified { token, account } = verified;
        if let Some(expected) = &self.expected
            && expected.account != account.id
        {
            self.window.set_error(
                format!(
                    "Kode ini untuk {}, bukan {} yang dipakai di komputer ini. Masuk dengan {}, \
                     atau keluar dari akun dulu.",
                    account.email, expected.email, expected.email
                )
                .into(),
            );
            self.window.set_step(0);
            // Sesi yang tidak dipakai diakhiri lagi.
            let api = HttpApi::new(&self.server, Some(token));
            in_background(move || api.logout(), |_| {});
            return;
        }
        if let Err(e) = save_token(&self.server, &self.data_dir, &token) {
            self.window.set_error(e.into());
            return;
        }
        // Daftar profil yang dihapus tetap dipakai bila akunnya sama.
        let removed = SignedIn::load(&self.data_dir)
            .ok()
            .flatten()
            .filter(|s| s.server == self.server && s.account == account.id)
            .map(|s| s.removed)
            .unwrap_or_default();
        let signed = SignedIn {
            server: self.server.clone(),
            account: account.id,
            email: account.email,
            removed,
        };
        if let Err(e) = signed.save(&self.data_dir) {
            forget_token(&self.server, &self.data_dir);
            self.window.set_error(e.into());
            return;
        }
        self.finish(Outcome::SignedIn(Box::new(Account::new(signed, token))));
    }

    fn change_email(&self) {
        self.window.set_step(0);
        self.window.set_code(Default::default());
        self.window.set_error(Default::default());
    }

    fn skip(&self) {
        self.finish(Outcome::Skipped);
    }

    fn cancel(&self) {
        self.finish(Outcome::Cancelled);
    }

    fn finish(&self, outcome: Outcome) {
        self.resend_timer.stop();
        let done = self.done.borrow_mut().take();
        if let (Some(done), Some(me)) = (done, self.me.upgrade()) {
            done(me, outcome);
        }
    }

    /// Langkah "Menyiapkan profil" setelah masuk pertama kali.
    fn preparing(&self) {
        self.window.set_error(Default::default());
        self.window.set_can_skip(false);
        self.window.set_step(2);
        self.set_busy(true);
    }
}

/// Lentera dibuka pertama kali dengan server tetapi belum masuk: layar masuk
/// dulu, lalu jendela operator. Jendela operator disimpan di `slot`.
pub fn sign_in_first(
    server: String,
    launch: Launch,
    slot: Rc<RefCell<Option<Rc<App>>>>,
) -> Result<Rc<Login>, String> {
    let dir = launch.dir.clone();
    let has_data = has_local_data(&dir);
    Login::new(
        server.clone(),
        dir.clone(),
        None,
        false,
        has_data,
        move |login, outcome| match outcome {
            Outcome::Cancelled => {
                let _ = slint::quit_event_loop();
            }
            Outcome::Skipped => open_after_login(&login, launch, Opening::SignedOut(server), &slot),
            Outcome::SignedIn(account) if has_data => {
                open_after_login(&login, launch, Opening::SignedIn(account), &slot)
            }
            Outcome::SignedIn(account) => {
                // Komputer baru: ambil profil akun lebih dulu, supaya Lentera
                // tidak membuat profil contoh yang ikut terkirim ke akun.
                login.preparing();
                let api = account.api.clone();
                let mut engine = Engine::new(dir, account.signed.account.clone(), None, Vec::new());
                in_background(
                    move || run(&mut engine, api.as_ref()),
                    move |()| open_after_login(&login, launch, Opening::SignedIn(account), &slot),
                );
            }
        },
    )
}

enum Opening {
    SignedIn(Box<Account>),
    SignedOut(String),
}

fn open_after_login(
    login: &Login,
    launch: Launch,
    opening: Opening,
    slot: &RefCell<Option<Rc<App>>>,
) {
    match open_app(launch) {
        Ok(app) => {
            match opening {
                Opening::SignedIn(account) => app.start_sync(*account),
                Opening::SignedOut(server) => app.signed_out(server),
            }
            *slot.borrow_mut() = Some(app);
            let _ = login.window.hide();
        }
        Err(e) => {
            let _ = login.window.hide();
            dialog(
                None,
                MessageLevel::Error,
                "Lentera tidak bisa dibuka",
                &e,
                MessageButtons::Ok,
            );
            let _ = slint::quit_event_loop();
        }
    }
}
