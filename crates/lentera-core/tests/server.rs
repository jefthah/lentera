//! Uji sinkron dengan server Lentera sungguhan (`server/`).
//!
//! Dilewati kecuali `LENTERA_TEST_SERVER` (misalnya `http://127.0.0.1:8310`)
//! dan `LENTERA_TEST_TOKEN` (token sesi akun uji) diisi. Lihat
//! `docs/PENGUJIAN.md` untuk cara menyiapkannya.
#![cfg(feature = "http")]

use std::{fs, path::PathBuf};

use lentera_core::sync::{Api, Engine, HttpApi, Stop, run};
use lentera_core::{Kind, Store, profiles};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("lentera-server-{}-{name}", std::process::id()));
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

fn server() -> Option<(String, String)> {
    let base = std::env::var("LENTERA_TEST_SERVER").ok()?;
    let token = std::env::var("LENTERA_TEST_TOKEN").ok()?;
    Some((base, token))
}

fn titles(dir: &std::path::Path, id: &str) -> (Vec<String>, Vec<String>, usize) {
    let library = Store::open_profile(dir, id).unwrap().library;
    let mut items: Vec<String> = library.items.iter().map(|i| i.title.clone()).collect();
    items.sort();
    let service = library
        .service
        .iter()
        .map(|id| library.item(*id).unwrap().title.clone())
        .collect();
    (items, service, library.background)
}

/// Nama dan judul isi setiap susunan tersimpan, urut nama.
fn saved(library: &lentera_core::Library) -> Vec<(String, Vec<String>)> {
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

#[test]
fn two_devices_share_a_profile_through_the_server() {
    let Some((base, token)) = server() else {
        eprintln!("LENTERA_TEST_SERVER / LENTERA_TEST_TOKEN kosong; uji dilewati");
        return;
    };
    let api = HttpApi::new(&base, Some(token));
    let account = api.me().expect("token uji tidak berlaku").id;
    let a = TempDir::new("a");
    let b = TempDir::new("b");
    let sync = |dir: &TempDir| {
        let mut engine = Engine::new(&dir.0, &account, None, Vec::new());
        run(&mut engine, &api);
        let report = engine.report().clone();
        assert_eq!(report.stop, None, "{report:?}");
        assert!(report.problems.is_empty(), "{report:?}");
    };

    // A: profil baru dengan lagu panjang, ayat, susunan, dan latar.
    let opened = Store::open(&a.0).unwrap();
    let id = opened.store.id().to_owned();
    let mut library = opened.library;
    let long = (1..=40)
        .map(|n| format!("Bait {n} ✓\nBaris kedua"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let song = library.add(Kind::Song, "Lagu uji panjang".into(), long.clone());
    library.service.push(song);
    library.background = 2;
    // Susunan tersimpan: isinya ikut, pilihan susunan tetap per komputer.
    let natal = library.create_service("Natal uji", vec![song, 1]).unwrap();
    library.create_service("Dihapus nanti", Vec::new()).unwrap();
    library.select_service(Some(natal));
    opened.store.save(&library).unwrap();
    drop(opened.store);
    sync(&a);
    assert!(api.profiles().unwrap().iter().any(|p| p.id == id));

    // B mengunduh profil itu.
    sync(&b);
    assert!(profiles(&b.0).unwrap().iter().any(|p| p.id == id));
    assert_eq!(titles(&a.0, &id), titles(&b.0, &id));
    let copy = Store::open_profile(&b.0, &id).unwrap();
    let mut lib_b = copy.library;
    let song_b = lib_b
        .items
        .iter()
        .find(|i| i.title == "Lagu uji panjang")
        .unwrap()
        .id;
    assert_eq!(lib_b.item(song_b).unwrap().content, long);
    assert_eq!(saved(&lib_b), saved(&library));
    assert_eq!(lib_b.active, None);

    // B mengubah lagu, menghapus ayat contoh, dan mengubah susunan.
    std::thread::sleep(std::time::Duration::from_millis(5));
    lib_b.update(song_b, "Lagu uji diubah di B", "Isi B");
    let verse = lib_b
        .items
        .iter()
        .find(|i| i.kind == Kind::Verse)
        .unwrap()
        .id;
    lib_b.remove(verse);
    lib_b.service.reverse();
    let natal_b = lib_b
        .services
        .iter()
        .find(|s| s.name == "Natal uji")
        .unwrap()
        .id;
    let other_b = lib_b
        .services
        .iter()
        .find(|s| s.name != "Natal uji")
        .unwrap()
        .id;
    lib_b.rename_service(natal_b, "Natal uji B").unwrap();
    lib_b.remove_service(other_b);
    copy.store.save(&lib_b).unwrap();
    drop(copy.store);
    sync(&b);
    sync(&a);
    let (items, service, background) = titles(&a.0, &id);
    assert_eq!(
        (items.clone(), service.clone(), background),
        titles(&b.0, &id)
    );
    assert!(
        items.contains(&"Lagu uji diubah di B".to_owned()),
        "{items:?}"
    );
    assert_eq!(items.len(), 2, "{items:?}");
    assert_eq!(
        service.first().map(String::as_str),
        Some("Lagu uji diubah di B")
    );
    assert_eq!(background, 2);
    let lib_a = Store::open_profile(&a.0, &id).unwrap().library;
    assert_eq!(
        saved(&lib_a),
        vec![(
            "Natal uji B".to_owned(),
            vec![
                "Lagu uji diubah di B".to_owned(),
                "Selamat datang di Lentera".to_owned()
            ]
        )]
    );
    assert_eq!(lib_a.active, Some(natal));

    // Rapikan: hapus profil uji di server.
    api.delete_profile(&id).unwrap();
}

#[test]
fn wrong_code_and_bad_token_get_clear_errors() {
    let Some((base, _)) = server() else {
        return;
    };
    let api = HttpApi::new(&base, Some("token-salah".into()));
    let e = api.me().unwrap_err();
    assert_eq!(e.status, 401, "{e:?}");
    let mut engine = Engine::new(std::env::temp_dir(), "akun", None, Vec::new());
    run(&mut engine, &api);
    assert_eq!(engine.report().stop, Some(Stop::LoginNeeded));

    let anonymous = HttpApi::new(&base, None);
    let e = anonymous
        .verify("belum-ada@lentera.test", "000000", "uji")
        .unwrap_err();
    assert_eq!(e.status, 422, "{e:?}");
    assert!(!e.message.is_empty());

    let offline = HttpApi::new("http://127.0.0.1:9", None);
    assert!(offline.me().unwrap_err().is_offline());
}
