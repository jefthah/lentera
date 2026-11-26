//! Uji sinkron dengan server tiruan di memori yang mengikuti aturan
//! `docs/SINKRON.md`: revisi per profil, perubahan terbaru menang, kiriman
//! sendiri tidak dikirim balik, dan jawaban dipecah per halaman.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{Kind, Library, Store, profiles};

const ACCOUNT: &str = "akun-1";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("lentera-sync-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn pause() {
    std::thread::sleep(std::time::Duration::from_millis(5));
}

#[derive(Clone, Default)]
struct FakeProfile {
    name: String,
    name_updated_at: i64,
    created_at: i64,
    owner: String,
    revision: i64,
    deleted: bool,
    items: HashMap<String, (RemoteItem, i64)>,
    service: Option<(RemoteService, i64)>,
    settings: Option<(RemoteSettings, i64)>,
    services: HashMap<String, (RemoteSavedService, i64)>,
}

#[derive(Default)]
struct FakeServer {
    profiles: RefCell<HashMap<String, FakeProfile>>,
    /// Materi per halaman jawaban.
    page: Cell<usize>,
    /// Galat untuk permintaan berikutnya.
    fail: RefCell<Option<ApiError>>,
    calls: Cell<usize>,
    /// Berlaku seperti server sebelum 0.9: tanpa susunan tersimpan.
    old: Cell<bool>,
}

fn api_error(status: u16, code: &str) -> ApiError {
    ApiError {
        status,
        code: code.into(),
        message: format!("galat {code}"),
        field: None,
        retry_after: None,
    }
}

impl FakeServer {
    fn new() -> Self {
        let server = Self::default();
        server.page.set(1000);
        server
    }

    fn take_failure(&self) -> Result<(), ApiError> {
        self.calls.set(self.calls.get() + 1);
        match self.fail.borrow_mut().take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn remote(id: &str, p: &FakeProfile) -> RemoteProfile {
        RemoteProfile {
            id: id.into(),
            name: p.name.clone(),
            role: if p.owner == ACCOUNT {
                "owner"
            } else {
                "editor"
            }
            .into(),
            created_at: p.created_at,
            name_updated_at: p.name_updated_at,
            revision: p.revision,
        }
    }

    fn live_titles(&self, id: &str) -> Vec<String> {
        let profiles = self.profiles.borrow();
        let mut titles: Vec<String> = profiles[id]
            .items
            .values()
            .filter(|(item, _)| item.deleted_at.is_none())
            .map(|(item, _)| item.title.clone())
            .collect();
        titles.sort();
        titles
    }
}

impl Api for FakeServer {
    fn profiles(&self) -> Result<Vec<RemoteProfile>, ApiError> {
        self.take_failure()?;
        let mut list: Vec<RemoteProfile> = self
            .profiles
            .borrow()
            .iter()
            .filter(|(_, p)| !p.deleted)
            .map(|(id, p)| Self::remote(id, p))
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(list)
    }

    fn put_profile(&self, id: &str, body: &ProfilePut) -> Result<RemoteProfile, ApiError> {
        self.take_failure()?;
        let mut profiles = self.profiles.borrow_mut();
        let profile = profiles.entry(id.into()).or_insert_with(|| FakeProfile {
            name: body.name.clone(),
            name_updated_at: body.updated_at,
            created_at: body.created_at,
            owner: ACCOUNT.into(),
            ..FakeProfile::default()
        });
        if profile.deleted {
            return Err(api_error(410, "profile_deleted"));
        }
        if body.updated_at > profile.name_updated_at {
            profile.name = body.name.clone();
            profile.name_updated_at = body.updated_at;
        }
        Ok(Self::remote(id, profile))
    }

    fn delete_profile(&self, id: &str) -> Result<(), ApiError> {
        self.take_failure()?;
        let mut profiles = self.profiles.borrow_mut();
        match profiles.get_mut(id) {
            Some(p) if !p.deleted => {
                p.deleted = true;
                Ok(())
            }
            Some(_) => Err(api_error(410, "profile_deleted")),
            None => Err(api_error(404, "profile_not_found")),
        }
    }

    fn sync(&self, id: &str, request: &SyncRequest) -> Result<SyncResponse, ApiError> {
        self.take_failure()?;
        let mut profiles = self.profiles.borrow_mut();
        let Some(p) = profiles.get_mut(id) else {
            return Err(api_error(404, "profile_not_found"));
        };
        if p.deleted {
            return Err(api_error(410, "profile_deleted"));
        }
        if request.cursor > p.revision {
            return Err(api_error(409, "cursor_ahead"));
        }
        // Seperti server sungguhan: satu materi berisi NUL membuat seluruh
        // kiriman ditolak.
        if request
            .items
            .iter()
            .any(|i| i.title.contains('\0') || i.content.contains('\0'))
        {
            return Err(api_error(422, "validation_failed"));
        }
        let mut own = Vec::new();
        let mut losers = Vec::new();
        for item in &request.items {
            let newer = p
                .items
                .get(&item.id)
                .is_none_or(|(old, _)| item.updated_at > old.updated_at);
            if newer {
                p.revision += 1;
                let mut stored = item.clone();
                if stored.deleted_at.is_some() {
                    stored.title.clear();
                    stored.content.clear();
                }
                p.items.insert(item.id.clone(), (stored, p.revision));
                own.push(item.id.clone());
            } else {
                losers.push(p.items[&item.id].0.clone());
            }
        }
        let mut service_back = None;
        if let Some(service) = &request.service {
            for uid in &service.items {
                let known = p
                    .items
                    .get(uid)
                    .is_some_and(|(item, _)| item.deleted_at.is_none());
                if !known {
                    return Err(api_error(422, "service_unknown_item"));
                }
            }
            match &p.service {
                Some((old, _)) if old.updated_at >= service.updated_at => {
                    service_back = Some(old.clone());
                }
                _ => {
                    p.revision += 1;
                    p.service = Some((service.clone(), p.revision));
                }
            }
        } else if let Some((service, rev)) = &p.service
            && *rev > request.cursor
        {
            service_back = Some(service.clone());
        }
        let mut settings_back = None;
        if let Some(settings) = &request.settings {
            match &p.settings {
                Some((old, _)) if old.updated_at >= settings.updated_at => {
                    settings_back = Some(old.clone());
                }
                _ => {
                    p.revision += 1;
                    p.settings = Some((settings.clone(), p.revision));
                }
            }
        } else if let Some((settings, rev)) = &p.settings
            && *rev > request.cursor
        {
            settings_back = Some(settings.clone());
        }
        let mut own_services = Vec::new();
        let mut services_back = Vec::new();
        let old = self.old.get();
        for service in request.services.iter().filter(|_| !old) {
            let unknown = service.items.iter().any(|uid| !p.items.contains_key(uid));
            if service.deleted_at.is_none() && unknown {
                return Err(api_error(422, "service_unknown_item"));
            }
            match p.services.get(&service.id) {
                Some((old, _)) if old.updated_at >= service.updated_at => {
                    services_back.push(old.clone());
                }
                _ => {
                    p.revision += 1;
                    p.services
                        .insert(service.id.clone(), (service.clone(), p.revision));
                    own_services.push(service.id.clone());
                }
            }
        }
        let mut changed: Vec<(RemoteItem, i64)> = p
            .items
            .values()
            .filter(|(item, rev)| *rev > request.cursor && !own.contains(&item.id))
            .cloned()
            .collect();
        changed.sort_by_key(|(_, rev)| *rev);
        let more = changed.len() > self.page.get();
        changed.truncate(self.page.get());
        let cursor = if more {
            changed.last().map_or(p.revision, |(_, rev)| *rev)
        } else {
            p.revision
        };
        let mut items: Vec<RemoteItem> = changed.into_iter().map(|(item, _)| item).collect();
        items.extend(losers);
        let mut services: Vec<RemoteSavedService> = p
            .services
            .values()
            .filter(|(service, rev)| {
                *rev > request.cursor && *rev <= cursor && !own_services.contains(&service.id)
            })
            .map(|(service, _)| service.clone())
            .collect();
        services.extend(services_back);
        Ok(SyncResponse {
            cursor,
            more,
            accepted: own.len() as i64,
            items,
            service: service_back,
            settings: settings_back,
            services: (!old).then_some(services),
            server_time: crate::store::now_ms(),
        })
    }
}

fn sync(dir: &Path, server: &FakeServer, first: Option<&str>) -> Report {
    sync_removed(dir, server, first, Vec::new())
}

fn sync_removed(
    dir: &Path,
    server: &FakeServer,
    first: Option<&str>,
    removed: Vec<String>,
) -> Report {
    let mut engine = Engine::new(dir, ACCOUNT, first.map(String::from), removed);
    run(&mut engine, server);
    engine.report().clone()
}

fn only_profile(dir: &Path) -> String {
    let list = profiles(dir).unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
    list[0].id.clone()
}

/// Judul materi di pustaka, berurutan, dan judul susunan ibadah.
fn titles(library: &Library) -> (Vec<String>, Vec<String>) {
    let mut items: Vec<String> = library.items.iter().map(|i| i.title.clone()).collect();
    items.sort();
    let service = library
        .service
        .iter()
        .map(|id| library.item(*id).unwrap().title.clone())
        .collect();
    (items, service)
}

fn open(dir: &Path, id: &str) -> (Store, Library) {
    let opened = Store::open_profile(dir, id).unwrap();
    (opened.store, opened.library)
}

#[test]
fn first_sync_uploads_local_profile_and_other_device_downloads_it() {
    let a = TempDir::new("up-a");
    let b = TempDir::new("up-b");
    let server = FakeServer::new();
    let opened = Store::open(&a.0).unwrap();
    let mut library = opened.library;
    library.add(
        Kind::Song,
        "Kasih-Mu".into(),
        "Bait satu\n\nBait dua".into(),
    );
    library.background = 1;
    opened.store.save(&library).unwrap();
    let id = opened.store.id().to_owned();
    drop(opened.store);

    let report = sync(&a.0, &server, Some(&id));
    assert_eq!(report.stop, None, "{report:?}");
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(
        server.live_titles(&id),
        vec![
            "Ayat contoh — ganti dengan referensi",
            "Kasih-Mu",
            "Selamat datang di Lentera"
        ]
    );

    let report = sync(&b.0, &server, None);
    assert_eq!(report.stop, None, "{report:?}");
    assert_eq!(only_profile(&b.0), id);
    let (store, copy) = open(&b.0, &id);
    assert_eq!(store.name(), "Profil utama");
    assert_eq!(titles(&copy), titles(&library));
    assert_eq!(copy.background, 1);
    let song = copy.items.iter().find(|i| i.title == "Kasih-Mu").unwrap();
    assert_eq!(song.content, "Bait satu\n\nBait dua");
}

#[test]
fn nul_in_old_imports_does_not_block_the_upload() {
    let a = TempDir::new("nul-a");
    let server = FakeServer::new();
    let opened = Store::open(&a.0).unwrap();
    let id = opened.store.id().to_owned();
    let path = opened.store.path().to_owned();
    drop(opened);
    // Lagu EasyWorship 7 yang diimpor Lentera 0.6, belum pernah dibuka lagi.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE items SET title = ?1, content = ?2 WHERE id = 1",
            ["TAHUN YOBEL\0", "S'bab ini tahun pembebasan\0"],
        )
        .unwrap();

    let report = sync(&a.0, &server, Some(&id));
    assert_eq!(report.stop, None, "{report:?}");
    assert_eq!(report.problems, Vec::<String>::new());
    let sent = |title: &str| {
        let profiles = server.profiles.borrow();
        let (item, _) = profiles[&id]
            .items
            .values()
            .find(|(item, _)| item.title == title)
            .cloned()
            .unwrap();
        item
    };
    assert_eq!(sent("TAHUN YOBEL").content, "S'bab ini tahun pembebasan");

    // Saat dibuka, profil juga dibersihkan di komputer ini dan versi bersih
    // dikirim lagi tanpa masalah.
    let (_, library) = open(&a.0, &id);
    assert_eq!(
        library.item(1).unwrap().content,
        "S'bab ini tahun pembebasan"
    );
    let report = sync(&a.0, &server, Some(&id));
    assert_eq!(report.stop, None, "{report:?}");
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(sent("TAHUN YOBEL").content, "S'bab ini tahun pembebasan");
}

#[test]
fn edits_deletes_and_service_travel_between_devices() {
    let a = TempDir::new("edit-a");
    let b = TempDir::new("edit-b");
    let server = FakeServer::new();
    let id = Store::open(&a.0).unwrap().store.id().to_owned();
    sync(&a.0, &server, None);
    sync(&b.0, &server, None);

    // B mengubah lagu pertama dan menghapus ayat contoh.
    pause();
    let (store_b, mut lib_b) = open(&b.0, &id);
    let song = lib_b
        .items
        .iter()
        .find(|i| i.kind == Kind::Song)
        .unwrap()
        .id;
    let verse = lib_b
        .items
        .iter()
        .find(|i| i.kind == Kind::Verse)
        .unwrap()
        .id;
    lib_b.update(song, "Lagu baru", "Isi baru");
    lib_b.remove(verse);
    store_b.save(&lib_b).unwrap();
    drop(store_b);
    sync(&b.0, &server, None);

    // A menambah lagu dan menaruhnya di susunan.
    pause();
    let (store_a, mut lib_a) = open(&a.0, &id);
    let added = lib_a.add(Kind::Song, "Dari A".into(), "A".into());
    lib_a.service.push(added);
    store_a.save(&lib_a).unwrap();
    drop(store_a);
    let report = sync(&a.0, &server, None);
    assert_eq!(report.stop, None, "{report:?}");
    sync(&b.0, &server, None);

    let (_, lib_a) = open(&a.0, &id);
    let (_, lib_b) = open(&b.0, &id);
    assert_eq!(titles(&lib_a), titles(&lib_b));
    assert_eq!(
        titles(&lib_a),
        (
            vec!["Dari A".to_owned(), "Lagu baru".to_owned()],
            vec!["Lagu baru".to_owned(), "Dari A".to_owned()]
        )
    );
}

#[test]
fn newest_edit_wins_when_both_devices_change_one_item() {
    let a = TempDir::new("conflict-a");
    let b = TempDir::new("conflict-b");
    let server = FakeServer::new();
    let id = Store::open(&a.0).unwrap().store.id().to_owned();
    sync(&a.0, &server, None);
    sync(&b.0, &server, None);

    let edit = |dir: &Path, title: &str| {
        let (store, mut library) = open(dir, &id);
        let song = library
            .items
            .iter()
            .find(|i| i.kind == Kind::Song)
            .unwrap()
            .id;
        library.update(song, title, "isi");
        store.save(&library).unwrap();
    };
    pause();
    edit(&a.0, "Versi A");
    pause();
    edit(&b.0, "Versi B (lebih baru)");
    // B mengirim lebih dulu, lalu A yang lebih lama kalah.
    sync(&b.0, &server, None);
    sync(&a.0, &server, None);
    sync(&b.0, &server, None);
    for dir in [&a.0, &b.0] {
        let (_, library) = open(dir, &id);
        assert!(
            library
                .items
                .iter()
                .any(|i| i.title == "Versi B (lebih baru)"),
            "{:?}",
            titles(&library)
        );
        assert!(!library.items.iter().any(|i| i.title == "Versi A"));
    }
}

#[test]
fn paged_download_applies_service_after_the_last_page() {
    let a = TempDir::new("page-a");
    let b = TempDir::new("page-b");
    let server = FakeServer::new();
    let opened = Store::open(&a.0).unwrap();
    let id = opened.store.id().to_owned();
    let mut library = opened.library;
    for n in 0..7 {
        let added = library.add(Kind::Song, format!("Lagu {n}"), "isi".into());
        library.service.push(added);
    }
    opened.store.save(&library).unwrap();
    sync(&a.0, &server, None);

    server.page.set(2);
    let calls = server.calls.get();
    let report = sync(&b.0, &server, None);
    assert_eq!(report.stop, None, "{report:?}");
    assert!(server.calls.get() - calls >= 6, "pages were not used");
    let (_, copy) = open(&b.0, &id);
    assert_eq!(titles(&copy), titles(&library));
}

/// Nama dan judul isi setiap susunan tersimpan, urut nama.
fn saved(library: &Library) -> Vec<(String, Vec<String>)> {
    let mut list: Vec<_> = library
        .services
        .iter()
        .map(|s| {
            let items = s
                .items
                .iter()
                .map(|id| library.item(*id).unwrap().title.clone())
                .collect();
            (s.name.clone(), items)
        })
        .collect();
    list.sort();
    list
}

fn names(list: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    list.iter()
        .map(|(name, items)| {
            (
                name.to_string(),
                items.iter().map(|t| t.to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn saved_services_travel_but_the_active_one_stays_on_each_computer() {
    let a = TempDir::new("saved-a");
    let b = TempDir::new("saved-b");
    let server = FakeServer::new();
    let opened = Store::open(&a.0).unwrap();
    let id = opened.store.id().to_owned();
    let mut lib_a = opened.library;
    let song = lib_a.add(Kind::Song, "Malam Kudus".into(), "Malam kudus".into());
    let natal = lib_a.create_service("Natal", vec![song, 1, song]).unwrap();
    lib_a.create_service("Kosong", Vec::new()).unwrap();
    lib_a.select_service(Some(natal));
    opened.store.save(&lib_a).unwrap();
    drop(opened.store);
    let report = sync(&a.0, &server, None);
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(report.stop, None, "{report:?}");

    let report = sync(&b.0, &server, None);
    assert_eq!(report.stop, None, "{report:?}");
    let (store_b, mut lib_b) = open(&b.0, &id);
    assert_eq!(saved(&lib_b), saved(&lib_a));
    // Susunan yang dipakai berlaku per komputer.
    assert_eq!(lib_b.active, None);
    assert_eq!(titles(&lib_b), titles(&lib_a));

    // B mengganti nama, menghapus satu materi, dan menghapus susunan kosong.
    pause();
    let natal_b = lib_b
        .services
        .iter()
        .find(|s| s.name == "Natal")
        .unwrap()
        .id;
    let kosong_b = lib_b
        .services
        .iter()
        .find(|s| s.name == "Kosong")
        .unwrap()
        .id;
    lib_b.rename_service(natal_b, "Ibadah Natal").unwrap();
    let first = lib_b
        .items
        .iter()
        .find(|i| i.title == "Selamat datang di Lentera")
        .unwrap()
        .id;
    lib_b.remove(first);
    lib_b.remove_service(kosong_b);
    lib_b.select_service(Some(natal_b));
    store_b.save(&lib_b).unwrap();
    drop(store_b);
    let report = sync(&b.0, &server, None);
    assert_eq!(report.stop, None, "{report:?}");
    assert_eq!(report.problems, Vec::<String>::new());

    sync(&a.0, &server, None);
    let (store_a, mut lib_a) = open(&a.0, &id);
    assert_eq!(
        saved(&lib_a),
        names(&[("Ibadah Natal", &["Malam Kudus", "Malam Kudus"])])
    );
    assert_eq!(lib_a.active, Some(natal));
    assert_eq!(lib_a.active_name(), "Ibadah Natal");

    // A menghapus susunan yang sedang dipakai B; B kembali ke Susunan utama.
    pause();
    lib_a.remove_service(natal);
    store_a.save(&lib_a).unwrap();
    drop(store_a);
    sync(&a.0, &server, None);
    sync(&b.0, &server, None);
    let (store_b, lib_b) = open(&b.0, &id);
    assert!(lib_b.services.is_empty());
    assert_eq!(lib_b.active, None);
    drop(store_b);

    // Tidak ada yang tertinggal: sinkron berikutnya hanya meminta daftar profil.
    let calls = server.calls.get();
    sync(&a.0, &server, None);
    sync(&b.0, &server, None);
    assert_eq!(server.calls.get() - calls, 2);
}

#[test]
fn newest_saved_service_wins_and_pages_are_applied_together() {
    let a = TempDir::new("saved-conflict-a");
    let b = TempDir::new("saved-conflict-b");
    let server = FakeServer::new();
    let opened = Store::open(&a.0).unwrap();
    let id = opened.store.id().to_owned();
    let mut library = opened.library;
    let mut songs = Vec::new();
    for n in 0..7 {
        songs.push(library.add(Kind::Song, format!("Lagu {n}"), "isi".into()));
    }
    library.create_service("Minggu", songs.clone()).unwrap();
    opened.store.save(&library).unwrap();
    drop(opened.store);
    sync(&a.0, &server, None);

    // Unduhan berhalaman: susunan memuat materi dari halaman terakhir.
    server.page.set(2);
    let report = sync(&b.0, &server, None);
    assert_eq!(report.stop, None, "{report:?}");
    let (_, lib_b) = open(&b.0, &id);
    assert_eq!(saved(&lib_b), saved(&library));
    server.page.set(1000);

    let edit = |dir: &Path, keep: usize| {
        let (store, mut library) = open(dir, &id);
        let minggu = library.services[0].id;
        library.select_service(Some(minggu));
        library.order_mut().truncate(keep);
        store.save(&library).unwrap();
    };
    pause();
    edit(&a.0, 3);
    pause();
    edit(&b.0, 1);
    // B mengirim lebih dulu, lalu versi A yang lebih lama kalah.
    sync(&b.0, &server, None);
    sync(&a.0, &server, None);
    for dir in [&a.0, &b.0] {
        let (_, library) = open(dir, &id);
        assert_eq!(saved(&library), names(&[("Minggu", &["Lagu 0"])]));
    }
}

#[test]
fn saved_services_wait_for_a_server_that_keeps_them() {
    let a = TempDir::new("saved-old-a");
    let b = TempDir::new("saved-old-b");
    let server = FakeServer::new();
    server.old.set(true);
    let opened = Store::open(&a.0).unwrap();
    let id = opened.store.id().to_owned();
    let mut library = opened.library;
    let song = library.add(Kind::Song, "Malam Kudus".into(), "Malam kudus".into());
    library.create_service("Natal", vec![song]).unwrap();
    library.service.push(song);
    opened.store.save(&library).unwrap();
    drop(opened.store);

    // Materi dan Susunan utama tetap terkirim; susunan tersimpan menunggu,
    // tanpa dikirim berulang dalam satu putaran.
    let calls = server.calls.get();
    let report = sync(&a.0, &server, None);
    assert_eq!(report.stop, None, "{report:?}");
    assert_eq!(report.problems.len(), 1, "{report:?}");
    assert!(report.problems[0].contains("belum mendukung"), "{report:?}");
    assert!(
        server.calls.get() - calls <= 4,
        "{}",
        server.calls.get() - calls
    );
    sync(&b.0, &server, None);
    let (_, lib_b) = open(&b.0, &id);
    assert_eq!(titles(&lib_b), titles(&library));
    assert!(lib_b.services.is_empty());

    // Setelah server diperbarui, susunan tersimpan ikut.
    server.old.set(false);
    let report = sync(&a.0, &server, None);
    assert_eq!(report.problems, Vec::<String>::new());
    sync(&b.0, &server, None);
    let (_, lib_b) = open(&b.0, &id);
    assert_eq!(saved(&lib_b), names(&[("Natal", &["Malam Kudus"])]));
}

#[test]
fn local_edit_made_during_a_request_is_kept_and_sent_next() {
    let a = TempDir::new("flight-a");
    let b = TempDir::new("flight-b");
    let server = FakeServer::new();
    let id = Store::open(&a.0).unwrap().store.id().to_owned();
    sync(&a.0, &server, None);
    sync(&b.0, &server, None);
    pause();
    let (store_b, mut lib_b) = open(&b.0, &id);
    let song = lib_b
        .items
        .iter()
        .find(|i| i.kind == Kind::Song)
        .unwrap()
        .id;
    lib_b.update(song, "Dari B", "b");
    store_b.save(&lib_b).unwrap();
    drop(store_b);
    sync(&b.0, &server, None);

    // A menarik versi B, tetapi selama permintaan berjalan operator di A
    // mengubah lagu yang sama.
    let (store_a, mut lib_a) = open(&a.0, &id);
    let mut engine = Engine::new(&a.0, ACCOUNT, None, Vec::new());
    let mut edited = false;
    while let Some(call) = engine.next_call() {
        let reply = perform(&server, &call);
        if !edited && matches!(call, Call::Sync { .. }) {
            pause();
            let song = lib_a
                .items
                .iter()
                .find(|i| i.kind == Kind::Song)
                .unwrap()
                .id;
            lib_a.update(song, "Diubah di A selama sinkron", "a");
            store_a.save(&lib_a).unwrap();
            edited = true;
        }
        engine.handle(reply);
    }
    let library = Store::open_profile(&a.0, &id).unwrap().library;
    assert!(
        library
            .items
            .iter()
            .any(|i| i.title == "Diubah di A selama sinkron"),
        "{:?}",
        titles(&library)
    );
    drop(store_a);
    sync(&a.0, &server, None);
    sync(&b.0, &server, None);
    let (_, lib_b) = open(&b.0, &id);
    assert!(
        lib_b
            .items
            .iter()
            .any(|i| i.title == "Diubah di A selama sinkron")
    );
}

#[test]
fn up_to_date_profiles_need_only_the_profile_list() {
    let a = TempDir::new("quiet-a");
    let server = FakeServer::new();
    Store::open(&a.0).unwrap();
    sync(&a.0, &server, None);
    let calls = server.calls.get();
    let report = sync(&a.0, &server, None);
    assert_eq!(report.synced, 1);
    assert_eq!(server.calls.get() - calls, 1);
}

#[test]
fn offline_and_expired_sessions_stop_the_round() {
    let a = TempDir::new("stop-a");
    let server = FakeServer::new();
    Store::open(&a.0).unwrap();
    *server.fail.borrow_mut() = Some(ApiError::offline("tidak ada jaringan"));
    let report = sync(&a.0, &server, None);
    assert!(matches!(report.stop, Some(Stop::Offline(_))), "{report:?}");
    *server.fail.borrow_mut() = Some(api_error(401, "unauthorized"));
    let report = sync(&a.0, &server, None);
    assert_eq!(report.stop, Some(Stop::LoginNeeded));
    let report = sync(&a.0, &server, None);
    assert_eq!(report.stop, None);
}

#[test]
fn server_restored_from_backup_receives_everything_again() {
    let a = TempDir::new("ahead-a");
    let server = FakeServer::new();
    let id = Store::open(&a.0).unwrap().store.id().to_owned();
    sync(&a.0, &server, None);
    // Server kembali ke keadaan kosong, tetapi profilnya masih ada.
    {
        let mut profiles = server.profiles.borrow_mut();
        let p = profiles.get_mut(&id).unwrap();
        p.items.clear();
        p.service = None;
        p.settings = None;
        p.revision = 0;
    }
    // Cursor perangkat sekarang lebih besar daripada revisi server.
    let report = sync(&a.0, &server, None);
    assert_eq!(report.stop, None, "{report:?}");
    assert_eq!(server.live_titles(&id).len(), 2);
    assert!(server.profiles.borrow()[&id].service.is_some());
}

#[test]
fn removed_profiles_are_deleted_on_the_server_and_not_downloaded_again() {
    let a = TempDir::new("removed-a");
    let b = TempDir::new("removed-b");
    let server = FakeServer::new();
    let first = Store::open(&a.0).unwrap();
    let keep = first.store.id().to_owned();
    let other = Store::create_profile(&a.0, "Pemuda", &first.library).unwrap();
    let gone = other.store.id().to_owned();
    drop((first, other));
    sync(&a.0, &server, None);
    assert_eq!(server.profiles().unwrap().len(), 2);

    crate::remove_profile(&a.0, &gone).unwrap();
    let report = sync_removed(&a.0, &server, None, vec![gone.clone()]);
    assert_eq!(report.stop, None, "{report:?}");
    let ids: Vec<String> = server
        .profiles()
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec![keep.clone()]);
    sync(&b.0, &server, None);
    assert_eq!(only_profile(&b.0), keep);
}

#[test]
fn renames_travel_and_clashing_names_get_a_number() {
    let a = TempDir::new("rename-a");
    let b = TempDir::new("rename-b");
    let server = FakeServer::new();
    let id = Store::open(&a.0).unwrap().store.id().to_owned();
    sync(&a.0, &server, None);
    // B sudah punya profil lokal bernama sama.
    let local_b = Store::open(&b.0).unwrap().store.id().to_owned();
    crate::rename_profile(&b.0, &local_b, "Ibadah Umum").unwrap();
    pause();
    crate::rename_profile(&a.0, &id, "Ibadah Umum").unwrap();
    sync(&a.0, &server, None);
    let report = sync(&b.0, &server, Some(&local_b));
    assert_eq!(report.stop, None, "{report:?}");
    let mut names: Vec<String> = profiles(&b.0)
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["Ibadah Umum", "Ibadah Umum (2)"]);
}

#[test]
fn deleted_and_foreign_profiles_are_left_alone() {
    let a = TempDir::new("foreign-a");
    let server = FakeServer::new();
    let first = Store::open(&a.0).unwrap();
    let id = first.store.id().to_owned();
    let other = Store::create_profile(&a.0, "Lain", &first.library).unwrap();
    let other_id = other.store.id().to_owned();
    drop((first, other));
    sync(&a.0, &server, None);

    server.delete_profile(&id).unwrap();
    let mut engine = Engine::new(&a.0, "akun-2", None, Vec::new());
    run(&mut engine, &server);
    let problems = engine.report().problems.join("\n");
    assert!(problems.contains("akun lain"), "{problems}");

    // Profil yang dihapus pemiliknya di server tidak disinkron lagi, tetapi
    // isinya tetap ada.
    let opened = Store::open_profile(&a.0, &id).unwrap();
    let mut library = opened.library;
    library.add(Kind::Song, "Baru".into(), "isi".into());
    opened.store.save(&library).unwrap();
    drop(opened.store);
    let report = sync(&a.0, &server, None);
    assert!(
        report.problems.iter().any(|p| p.contains("dihapus")),
        "{report:?}"
    );
    // Tetap dilaporkan di putaran berikutnya, tanpa permintaan untuk profil itu.
    let calls = server.calls.get();
    let report = sync(&a.0, &server, None);
    assert_eq!(report.problems.len(), 1, "{report:?}");
    assert!(report.problems[0].contains("dihapus"), "{report:?}");
    assert_eq!(server.calls.get() - calls, 1, "hanya daftar profil");
    assert_eq!(
        Store::open_profile(&a.0, &id).unwrap().library.items.len(),
        3
    );
    assert!(server.profiles.borrow().contains_key(&other_id));
}

#[test]
fn signed_in_file_round_trips_and_forgets() {
    let a = TempDir::new("account");
    assert_eq!(SignedIn::load(&a.0).unwrap(), None);
    let mut signed = SignedIn {
        server: "https://contoh.test".into(),
        account: ACCOUNT.into(),
        email: "operator@contoh.test".into(),
        removed: Vec::new(),
    };
    signed.remember_removed("x");
    signed.remember_removed("x");
    signed.save(&a.0).unwrap();
    assert_eq!(SignedIn::load(&a.0).unwrap(), Some(signed));
    SignedIn::forget(&a.0).unwrap();
    SignedIn::forget(&a.0).unwrap();
    assert_eq!(SignedIn::load(&a.0).unwrap(), None);
}
