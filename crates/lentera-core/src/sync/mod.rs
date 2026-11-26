//! Login dan sinkron profil dengan server Lentera (`server/` di repositori,
//! protokolnya di `docs/SINKRON.md`).
//!
//! Lentera tetap offline lebih dulu: semua data ada di SQLite per profil, dan
//! server hanya tempat bertukar perubahan. [`Engine`] menjalankan satu putaran
//! sinkron sebagai urutan [`Call`] ke server. Pemanggil yang mengirimnya,
//! sehingga aplikasi bisa melakukan permintaan HTTP di thread lain sementara
//! basis data tetap diubah di thread tampilan:
//!
//! ```text
//! while let Some(call) = engine.next_call() {
//!     let reply = perform(&api, &call);   // boleh di thread lain
//!     let effect = engine.handle(reply);  // menulis ke SQLite
//! }
//! ```
//!
//! Satu putaran: ambil daftar profil akun, hapus di server profil yang sudah
//! dihapus di perangkat ini, buat profil yang baru ada di server, lalu untuk
//! setiap profil: sambungkan atau kirim nama baru, tarik perubahan, dan kirim
//! perubahan lokal.

mod account;
mod db;
#[cfg(feature = "http")]
mod http;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

pub use account::{ACCOUNT_FILE, SignedIn};
use db::{Page, SyncDb};
#[cfg(feature = "http")]
pub use http::{HttpApi, device_name};

/// Batas server (lihat `docs/SINKRON.md`).
pub const MAX_TITLE_CHARS: usize = 1000;
pub const MAX_CONTENT_BYTES: usize = 1 << 20;
/// Materi per permintaan sinkron.
pub const MAX_ITEMS_PER_REQUEST: usize = 1000;
/// Susunan tersimpan per permintaan sinkron.
pub const MAX_SERVICES_PER_REQUEST: usize = 100;
/// Materi per susunan ibadah.
pub const MAX_SERVICE_ITEMS: usize = 1000;
/// Ukuran kiriman per permintaan; server menerima sampai 16 MB.
pub const MAX_REQUEST_BYTES: usize = 8 << 20;
/// Pengaman supaya satu profil tidak berputar terus bila ada yang tidak
/// pernah tercatat terkirim.
const MAX_ROUNDS: u32 = 200;

/// Materi seperti dikirim dan diterima server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteItem {
    /// `uid` materi (UUID v7).
    pub id: String,
    /// `song` atau `verse`.
    pub kind: String,
    pub title: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteService {
    /// `uid` materi, berurutan; boleh berulang.
    pub items: Vec<String>,
    pub updated_at: i64,
}

/// Susunan tersimpan seperti dikirim dan diterima server. Susunan yang
/// dihapus tidak punya nama dan isi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSavedService {
    /// `uid` susunan (UUID v7).
    pub id: String,
    pub name: String,
    /// `uid` materi, berurutan; boleh berulang.
    pub items: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSettings {
    pub background: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub cursor: i64,
    pub items: Vec<RemoteItem>,
    pub service: Option<RemoteService>,
    pub settings: Option<RemoteSettings>,
    /// Susunan tersimpan. Tidak ditulis bila kosong, jadi permintaan tanpa
    /// susunan tersimpan sama seperti Lentera 0.8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<RemoteSavedService>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncResponse {
    pub cursor: i64,
    #[serde(default)]
    pub more: bool,
    #[serde(default)]
    pub items: Vec<RemoteItem>,
    pub service: Option<RemoteService>,
    pub settings: Option<RemoteSettings>,
    /// Susunan tersimpan. Server sebelum Lentera 0.9 tidak mengirimnya
    /// (`None`), dan juga tidak menyimpan susunan tersimpan yang dikirim.
    #[serde(default)]
    pub services: Option<Vec<RemoteSavedService>>,
    #[serde(default)]
    pub accepted: i64,
    #[serde(default)]
    pub server_time: i64,
}

/// Profil seperti tersimpan di server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteProfile {
    pub id: String,
    pub name: String,
    /// `owner`, `editor`, atau `viewer`.
    pub role: String,
    pub created_at: i64,
    pub name_updated_at: i64,
    pub revision: i64,
}

/// Isi `PUT /v1/profiles/{id}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfilePut {
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Akun yang sedang masuk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAccount {
    /// Server mengirim angka; disimpan sebagai teks.
    #[serde(deserialize_with = "id_text")]
    pub id: String,
    pub email: String,
}

fn id_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Number(i64),
        Text(String),
    }
    Ok(match Id::deserialize(d)? {
        Id::Number(n) => n.to_string(),
        Id::Text(text) => text,
    })
}

/// Jawaban `POST /v1/auth/code`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeSent {
    pub email: String,
    pub sent: bool,
    pub seconds_until_resend: i64,
    pub expires_in_seconds: i64,
}

/// Jawaban `POST /v1/auth/verify`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verified {
    pub token: String,
    pub account: RemoteAccount,
}

/// Galat dari server, atau server tidak terjangkau (`status` 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: String,
    /// Pesan berbahasa Indonesia yang boleh ditampilkan apa adanya.
    pub message: String,
    pub field: Option<String>,
    /// Detik dari header `Retry-After` (status 429).
    pub retry_after: Option<u64>,
}

impl ApiError {
    pub fn offline(detail: impl std::fmt::Display) -> Self {
        Self {
            status: 0,
            code: "offline".into(),
            message: format!("Server Lentera tidak terjangkau. Periksa internet.\n{detail}"),
            field: None,
            retry_after: None,
        }
    }

    pub fn is_offline(&self) -> bool {
        self.status == 0
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Permintaan ke server yang dipakai sinkron. Login ada di
/// [`HttpApi`](http::HttpApi) saja.
pub trait Api {
    fn profiles(&self) -> Result<Vec<RemoteProfile>, ApiError>;
    fn put_profile(&self, id: &str, body: &ProfilePut) -> Result<RemoteProfile, ApiError>;
    fn delete_profile(&self, id: &str) -> Result<(), ApiError>;
    fn sync(&self, id: &str, request: &SyncRequest) -> Result<SyncResponse, ApiError>;
}

/// Satu permintaan yang diminta [`Engine`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    Profiles,
    PutProfile { id: String, body: ProfilePut },
    DeleteProfile { id: String },
    Sync { id: String, request: SyncRequest },
}

/// Jawaban untuk [`Call`] yang sama.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Profiles(Vec<RemoteProfile>),
    Profile(RemoteProfile),
    Deleted,
    Synced(SyncResponse),
}

/// Mengirim satu [`Call`] lewat `api`.
pub fn perform(api: &dyn Api, call: &Call) -> Result<Reply, ApiError> {
    match call {
        Call::Profiles => api.profiles().map(Reply::Profiles),
        Call::PutProfile { id, body } => api.put_profile(id, body).map(Reply::Profile),
        Call::DeleteProfile { id } => api.delete_profile(id).map(|()| Reply::Deleted),
        Call::Sync { id, request } => api.sync(id, request).map(Reply::Synced),
    }
}

/// Menjalankan satu putaran sinkron sampai selesai di thread ini.
pub fn run(engine: &mut Engine, api: &dyn Api) {
    while let Some(call) = engine.next_call() {
        let reply = perform(api, &call);
        engine.handle(reply);
    }
}

/// Alasan putaran sinkron berhenti sebelum selesai.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Server tidak terjangkau.
    Offline(String),
    /// Sesi berakhir; perlu masuk lagi. Data lokal tetap dipakai.
    LoginNeeded,
    /// Server sibuk; coba lagi setelah sekian detik.
    Wait(u64),
    /// Galat lain dari server.
    Failed(String),
}

/// Hasil satu putaran.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Profil yang selesai disinkron tanpa masalah.
    pub synced: usize,
    /// Pesan untuk operator, misalnya profil yang tidak bisa disinkron.
    pub problems: Vec<String>,
    pub stop: Option<Stop>,
    /// Jam server dari jawaban terakhir (milidetik, UTC).
    pub server_time: Option<i64>,
}

/// Akibat satu jawaban bagi aplikasi.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effect {
    /// Isi profil ini berubah (materi, susunan, latar, atau nama): baca
    /// ulang bila sedang dibuka.
    pub changed: Option<String>,
    /// Daftar profil di perangkat ini berubah (profil baru atau nama baru).
    pub profiles: bool,
}

enum Phase {
    List,
    Delete,
    Next,
    Profile(Box<Run>),
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Link,
    Pull,
    Push,
}

#[derive(Default)]
struct Sent {
    items: Vec<RemoteItem>,
    service: Option<i64>,
    settings: Option<i64>,
    /// `uid` dan `updated_at` susunan tersimpan yang dikirim.
    services: Vec<(String, i64)>,
    name_at: i64,
}

/// Sinkron satu profil.
struct Run {
    id: String,
    name: String,
    db: SyncDb,
    step: Step,
    read_only: bool,
    reset: bool,
    skip: HashSet<String>,
    limit: usize,
    service: Option<RemoteService>,
    settings: Option<RemoteSettings>,
    /// Susunan tersimpan dari halaman jawaban yang sudah diterima; diterapkan
    /// setelah halaman terakhir, karena bisa memuat materi di halaman berikutnya.
    services: Vec<RemoteSavedService>,
    /// Server belum menyimpan susunan tersimpan; jangan kirim lagi di putaran
    /// ini. Susunannya tetap belum tersinkron dan dikirim di sinkron berikutnya.
    old_server: bool,
    sent: Option<Sent>,
    rounds: u32,
}

enum Outcome {
    Continue,
    Finished,
    Abandoned,
    Stop(Stop),
}

/// Satu putaran sinkron untuk semua profil akun di folder data.
pub struct Engine {
    data_dir: PathBuf,
    account: String,
    first: Option<String>,
    removed: Vec<String>,
    phase: Phase,
    queue: VecDeque<String>,
    deletions: VecDeque<String>,
    remote: HashMap<String, RemoteProfile>,
    report: Report,
}

impl Engine {
    /// `account` adalah id akun yang sedang masuk. `first` disinkron lebih
    /// dulu (biasanya profil yang sedang dibuka). `removed` adalah profil yang
    /// sudah dihapus di perangkat ini (lihat [`SignedIn::removed`]).
    pub fn new(
        data_dir: impl Into<PathBuf>,
        account: impl Into<String>,
        first: Option<String>,
        removed: Vec<String>,
    ) -> Self {
        Self {
            data_dir: data_dir.into(),
            account: account.into(),
            first,
            removed,
            phase: Phase::List,
            queue: VecDeque::new(),
            deletions: VecDeque::new(),
            remote: HashMap::new(),
            report: Report::default(),
        }
    }

    pub fn report(&self) -> &Report {
        &self.report
    }

    pub fn is_done(&self) -> bool {
        matches!(self.phase, Phase::Done)
    }

    /// Permintaan berikutnya, atau `None` bila putaran sudah selesai.
    pub fn next_call(&mut self) -> Option<Call> {
        loop {
            match &mut self.phase {
                Phase::List => return Some(Call::Profiles),
                Phase::Delete => match self.deletions.front() {
                    Some(id) => return Some(Call::DeleteProfile { id: id.clone() }),
                    None => self.phase = Phase::Next,
                },
                Phase::Next => match self.queue.pop_front() {
                    Some(id) => {
                        if let Some(run) = self.start(&id) {
                            self.phase = Phase::Profile(Box::new(run));
                        }
                    }
                    None => self.phase = Phase::Done,
                },
                Phase::Profile(run) => match run.request(&mut self.report) {
                    Ok(Some(call)) => return Some(call),
                    Ok(None) => {
                        self.report.synced += 1;
                        self.phase = Phase::Next;
                    }
                    Err(e) => {
                        let name = run.name.clone();
                        self.problem(format!("Profil \"{name}\" tidak bisa disinkron.\n{e}"));
                        self.phase = Phase::Next;
                    }
                },
                Phase::Done => return None,
            }
        }
    }

    /// Menerapkan jawaban untuk permintaan terakhir dari [`Engine::next_call`].
    pub fn handle(&mut self, reply: Result<Reply, ApiError>) -> Effect {
        let mut effect = Effect::default();
        match std::mem::replace(&mut self.phase, Phase::Done) {
            Phase::List => match reply {
                Ok(Reply::Profiles(list)) => {
                    effect.profiles = self.plan(list);
                    self.phase = Phase::Delete;
                }
                Ok(_) => self.stop(Stop::Failed("Jawaban server tidak dikenal.".into())),
                Err(e) => self.stop(stop_for(&e)),
            },
            Phase::Delete => {
                match reply {
                    Ok(_) => {}
                    // Sudah tidak ada, atau akun ini bukan pemiliknya: cukup
                    // tidak disinkron lagi di perangkat ini.
                    Err(e) if matches!(e.status, 403 | 404 | 410) => {}
                    Err(e) => return self.stopped(stop_for(&e), effect),
                }
                self.deletions.pop_front();
                self.phase = Phase::Delete;
            }
            Phase::Profile(mut run) => {
                let data_dir = self.data_dir.clone();
                let account = self.account.clone();
                match run.handle(reply, &data_dir, &account, &mut effect, &mut self.report) {
                    Ok(Outcome::Continue) => self.phase = Phase::Profile(run),
                    Ok(Outcome::Finished) => {
                        self.report.synced += 1;
                        self.phase = Phase::Next;
                    }
                    Ok(Outcome::Abandoned) => self.phase = Phase::Next,
                    Ok(Outcome::Stop(stop)) => self.stop(stop),
                    Err(e) => {
                        self.problem(format!(
                            "Profil \"{}\" tidak bisa disinkron.\n{e}",
                            run.name
                        ));
                        self.phase = Phase::Next;
                    }
                }
            }
            Phase::Next | Phase::Done => {}
        }
        effect
    }

    fn stopped(&mut self, stop: Stop, effect: Effect) -> Effect {
        self.stop(stop);
        effect
    }

    fn stop(&mut self, stop: Stop) {
        self.report.stop = Some(stop);
        self.phase = Phase::Done;
    }

    fn problem(&mut self, message: String) {
        if !self.report.problems.contains(&message) {
            self.report.problems.push(message);
        }
    }

    /// Menyusun putaran dari daftar profil akun. Mengembalikan `true` bila
    /// ada profil baru di perangkat ini.
    fn plan(&mut self, list: Vec<RemoteProfile>) -> bool {
        let local: Vec<String> = match crate::profiles(&self.data_dir) {
            Ok(list) => list.into_iter().map(|p| p.id).collect(),
            Err(e) => {
                self.stop(Stop::Failed(e));
                return false;
            }
        };
        let mut created = false;
        for remote in &list {
            if self.removed.contains(&remote.id) {
                if remote.role == "owner" {
                    self.deletions.push_back(remote.id.clone());
                }
                continue;
            }
            if !local.contains(&remote.id) {
                match db::create_remote_profile(&self.data_dir, &self.account, remote) {
                    Ok(()) => {
                        created = true;
                        self.queue.push_back(remote.id.clone());
                    }
                    Err(e) => self.problem(format!(
                        "Profil \"{}\" dari server tidak bisa dibuat di komputer ini.\n{e}",
                        remote.name
                    )),
                }
            }
        }
        let mut order: Vec<String> = local
            .into_iter()
            .filter(|id| !self.removed.contains(id))
            .collect();
        if let Some(first) = &self.first
            && let Some(at) = order.iter().position(|id| id == first)
        {
            let first = order.remove(at);
            order.insert(0, first);
        }
        // Profil baru dari server sudah ada di antrean; profil lokal lebih dulu.
        for id in order.into_iter().rev() {
            self.queue.push_front(id);
        }
        self.remote = list.into_iter().map(|p| (p.id.clone(), p)).collect();
        created
    }

    /// Menyiapkan sinkron satu profil, atau `None` bila profil ini dilewati.
    fn start(&mut self, id: &str) -> Option<Run> {
        let opened = SyncDb::open(&self.data_dir, id).and_then(|db| db.info().map(|i| (db, i)));
        let (db, info) = match opened {
            Ok(found) => found,
            Err(e) => {
                self.problem(format!("Profil {id} tidak bisa disinkron.\n{e}"));
                return None;
            }
        };
        if !info.state.is_empty() {
            // Dilaporkan setiap putaran sampai profil ini dihapus di komputer
            // ini, supaya operator sempat melihatnya.
            self.problem(detached(&info.name, &info.state));
            return None;
        }
        if !info.account.is_empty() && info.account != self.account {
            self.problem(format!(
                "Profil \"{}\" tersambung ke akun lain, jadi tidak disinkron dengan akun ini.",
                info.name
            ));
            return None;
        }
        let remote = self.remote.get(id);
        let linked = info.account == self.account;
        let read_only = remote.is_some_and(|r| r.role == "viewer");
        let up_to_date = linked
            && remote.is_some_and(|r| r.revision == info.cursor)
            && matches!(db.has_changes(), Ok(false));
        if up_to_date {
            self.report.synced += 1;
            return None;
        }
        let rename = info.name_at != info.name_synced && !read_only;
        Some(Run {
            id: id.into(),
            name: info.name,
            db,
            step: if !linked || rename {
                Step::Link
            } else {
                Step::Pull
            },
            read_only,
            reset: false,
            skip: HashSet::new(),
            limit: MAX_ITEMS_PER_REQUEST,
            service: None,
            settings: None,
            services: Vec::new(),
            old_server: false,
            sent: None,
            rounds: 0,
        })
    }
}

impl Run {
    fn request(&mut self, report: &mut Report) -> Result<Option<Call>, String> {
        self.rounds += 1;
        if self.rounds > MAX_ROUNDS {
            return Err("Sinkron berhenti karena terlalu banyak putaran.".into());
        }
        match self.step {
            Step::Link => {
                let info = self.db.info()?;
                self.sent = Some(Sent {
                    name_at: info.name_at,
                    ..Sent::default()
                });
                Ok(Some(Call::PutProfile {
                    id: self.id.clone(),
                    body: ProfilePut {
                        name: info.name,
                        created_at: info.created_at,
                        updated_at: info.name_at,
                    },
                }))
            }
            Step::Pull => {
                self.sent = None;
                Ok(Some(self.sync_call(Vec::new(), None, None, Vec::new())?))
            }
            Step::Push => {
                if self.read_only {
                    return Ok(None);
                }
                let chunk = loop {
                    let chunk = self
                        .db
                        .dirty_items(self.limit, MAX_REQUEST_BYTES, &self.skip)?;
                    if chunk.oversize.is_empty() {
                        break chunk;
                    }
                    let titles: Vec<String> = chunk
                        .oversize
                        .iter()
                        .map(|(_, title)| format!("\"{title}\""))
                        .collect();
                    self.skip
                        .extend(chunk.oversize.into_iter().map(|(uid, _)| uid));
                    let message = format!(
                        "Materi terlalu besar untuk server (judul lebih dari {MAX_TITLE_CHARS} \
                         karakter atau isi lebih dari 1 MB), jadi tidak disinkron: {}.",
                        titles.join(", ")
                    );
                    if !report.problems.contains(&message) {
                        report.problems.push(message);
                    }
                };
                // Susunan dikirim bersama bagian materi terakhir, karena server
                // hanya menerima susunan berisi materi yang sudah ada di sana.
                let (service, settings, services) = if chunk.more {
                    (None, None, Vec::new())
                } else if self.old_server {
                    (
                        self.db.dirty_service(&self.skip)?,
                        self.db.dirty_settings()?,
                        Vec::new(),
                    )
                } else {
                    (
                        self.db.dirty_service(&self.skip)?,
                        self.db.dirty_settings()?,
                        self.db
                            .dirty_services(MAX_SERVICES_PER_REQUEST, &self.skip)?,
                    )
                };
                if chunk.items.is_empty()
                    && service.is_none()
                    && settings.is_none()
                    && services.is_empty()
                {
                    return Ok(None);
                }
                self.sent = Some(Sent {
                    items: chunk.items.clone(),
                    service: service.as_ref().map(|s| s.updated_at),
                    settings: settings.as_ref().map(|s| s.updated_at),
                    services: services
                        .iter()
                        .map(|s| (s.id.clone(), s.updated_at))
                        .collect(),
                    name_at: 0,
                });
                Ok(Some(self.sync_call(
                    chunk.items,
                    service,
                    settings,
                    services,
                )?))
            }
        }
    }

    fn sync_call(
        &self,
        items: Vec<RemoteItem>,
        service: Option<RemoteService>,
        settings: Option<RemoteSettings>,
        services: Vec<RemoteSavedService>,
    ) -> Result<Call, String> {
        Ok(Call::Sync {
            id: self.id.clone(),
            request: SyncRequest {
                cursor: self.db.info()?.cursor,
                items,
                service,
                settings,
                services,
            },
        })
    }

    fn handle(
        &mut self,
        reply: Result<Reply, ApiError>,
        data_dir: &std::path::Path,
        account: &str,
        effect: &mut Effect,
        report: &mut Report,
    ) -> Result<Outcome, String> {
        let sent = self.sent.take();
        let reply = match reply {
            Ok(reply) => reply,
            Err(e) => return self.failed(e, sent, report),
        };
        match (self.step, reply) {
            (Step::Link, Reply::Profile(remote)) => {
                let name_at = sent.map_or(0, |s| s.name_at);
                self.db.link(account, name_at)?;
                if remote.name != self.name
                    && self
                        .db
                        .set_name(data_dir, &self.id, &remote.name, remote.name_updated_at)?
                {
                    self.name = self.db.info()?.name;
                    effect.changed = Some(self.id.clone());
                    effect.profiles = true;
                }
                self.read_only = remote.role == "viewer";
                self.step = Step::Pull;
                Ok(Outcome::Continue)
            }
            (Step::Pull | Step::Push, Reply::Synced(response)) => {
                report.server_time = Some(response.server_time);
                if response.service.is_some() {
                    self.service = response.service;
                }
                if response.settings.is_some() {
                    self.settings = response.settings;
                }
                let old_server = response.services.is_none();
                for service in response.services.unwrap_or_default() {
                    self.services.retain(|s| s.id != service.id);
                    self.services.push(service);
                }
                let last = !response.more;
                let (service, settings, services) = if last {
                    (
                        self.service.take(),
                        self.settings.take(),
                        std::mem::take(&mut self.services),
                    )
                } else {
                    (None, None, Vec::new())
                };
                let mut sent = sent.unwrap_or_default();
                if old_server && !sent.services.is_empty() {
                    sent.services.clear();
                    self.old_server = true;
                    let message = "Server Lentera belum mendukung susunan tersimpan. \
                                   Susunan selain Susunan utama dikirim setelah server diperbarui."
                        .to_string();
                    if !report.problems.contains(&message) {
                        report.problems.push(message);
                    }
                }
                let changed = self.db.apply(&Page {
                    sent: &sent.items,
                    sent_service: sent.service,
                    sent_settings: sent.settings,
                    sent_services: &sent.services,
                    items: &response.items,
                    service: service.as_ref(),
                    settings: settings.as_ref(),
                    services: &services,
                    cursor: response.cursor,
                })?;
                if changed {
                    effect.changed = Some(self.id.clone());
                }
                self.step = if last { Step::Push } else { Step::Pull };
                Ok(Outcome::Continue)
            }
            _ => Ok(Outcome::Stop(Stop::Failed(
                "Jawaban server tidak dikenal.".into(),
            ))),
        }
    }

    fn failed(
        &mut self,
        e: ApiError,
        sent: Option<Sent>,
        report: &mut Report,
    ) -> Result<Outcome, String> {
        let name = &self.name;
        let mut problem = |message: String| {
            if !report.problems.contains(&message) {
                report.problems.push(message);
            }
        };
        match (e.status, e.code.as_str()) {
            (409, "cursor_ahead") if !self.reset => {
                self.db.reset()?;
                self.reset = true;
                self.step = Step::Pull;
                Ok(Outcome::Continue)
            }
            (410, _) => {
                self.db.set_state(DELETED)?;
                problem(detached(name, DELETED));
                Ok(Outcome::Abandoned)
            }
            (404, _) => {
                self.db.set_state(REMOVED)?;
                problem(detached(name, REMOVED));
                Ok(Outcome::Abandoned)
            }
            (403, "read_only") => {
                self.read_only = true;
                if self.step == Step::Link {
                    let at = sent.map_or(0, |s| s.name_at);
                    self.db.name_synced(at)?;
                    self.step = Step::Pull;
                    return Ok(Outcome::Continue);
                }
                Ok(Outcome::Finished)
            }
            (413, _) if self.limit > 1 => {
                self.limit = (self.limit / 2).max(1);
                Ok(Outcome::Continue)
            }
            (413, _) => {
                if let Some(sent) = sent {
                    self.skip.extend(sent.items.into_iter().map(|item| item.id));
                }
                problem(format!(
                    "Sebagian materi profil \"{name}\" terlalu besar untuk server."
                ));
                Ok(Outcome::Continue)
            }
            (409, _) | (422, _) | (403, _) => {
                problem(format!(
                    "Profil \"{name}\" tidak bisa disinkron.\n{}",
                    e.message
                ));
                Ok(Outcome::Abandoned)
            }
            _ => Ok(Outcome::Stop(stop_for(&e))),
        }
    }
}

/// `sync_state` profil yang sudah dihapus pemiliknya di server.
const DELETED: &str = "deleted";
/// `sync_state` profil yang tidak lagi bisa dibuka akun ini di server.
const REMOVED: &str = "removed";

/// Pesan untuk profil yang tidak disinkron lagi (lihat `sync_state`).
fn detached(name: &str, state: &str) -> String {
    let why = if state == DELETED {
        "sudah dihapus dari akun"
    } else {
        "tidak bisa dibuka lagi oleh akun ini"
    };
    format!(
        "Profil \"{name}\" {why}, jadi tidak disinkron lagi. Isinya masih ada di komputer ini; \
         hapus lewat Pilih profil bila tidak dipakai lagi."
    )
}

fn stop_for(e: &ApiError) -> Stop {
    match e.status {
        0 => Stop::Offline(e.message.clone()),
        401 => Stop::LoginNeeded,
        429 => Stop::Wait(e.retry_after.unwrap_or(60)),
        _ => Stop::Failed(e.message.clone()),
    }
}

#[cfg(test)]
mod tests;
