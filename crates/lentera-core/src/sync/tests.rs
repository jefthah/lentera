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
}

#[derive(Default)]
struct FakeServer {
    profiles: RefCell<HashMap<String, FakeProfile>>,
    /// Materi per halaman jawaban.
    page: Cell<usize>,
    /// Galat untuk permintaan berikutnya.
    fail: RefCell<Option<ApiError>>,
    calls: Cell<usize>,
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
        Ok(SyncResponse {
            cursor,
            more,
            accepted: own.len() as i64,
            items,
            service: service_back,
            settings: settings_back,
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
