//! Penyimpanan pustaka per profil dengan SQLite.
//!
//! Susunan folder data:
//!
//! ```text
//! <folder data>/
//!   library.lock                 kunci satu proses (nama sama dengan versi lama)
//!   library.json                 data Lentera ≤ 0.2.x; setelah dipindahkan ke
//!                                profil pertama, berkas ini dibiarkan sebagai cadangan
//!   profiles/<id profil>/library.db
//! ```
//!
//! [`Item::id`](crate::Item) adalah id lokal yang hanya berlaku di perangkat
//! ini, sehingga kode tampilan tidak berubah. Untuk sinkron nanti, setiap
//! materi juga punya `uid` (UUID v7) beserta waktu dibuat, diubah, dan
//! dihapus. Materi yang dihapus disimpan sebagai tanda hapus tanpa judul dan
//! isi, supaya penghapusan bisa ikut tersinkron dan id lokalnya tidak dipakai
//! ulang.

use crate::{Kind, LIBRARY_FILE, Library};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

/// Folder berisi satu subfolder per profil.
pub const PROFILES_DIR: &str = "profiles";
/// Nama berkas basis data di folder profil.
pub const PROFILE_DB: &str = "library.db";
/// Nama profil yang dibuat otomatis saat pertama kali dibuka.
pub const DEFAULT_PROFILE_NAME: &str = "Profil utama";
/// Folder tujuan profil yang dihapus. Isinya tidak dibaca Lentera lagi, tetapi
/// masih bisa dipulihkan dengan memindahkannya kembali ke [`PROFILES_DIR`].
pub const DELETED_PROFILES_DIR: &str = "profiles-deleted";
/// Panjang maksimum nama profil, dalam karakter.
pub const PROFILE_NAME_MAX: usize = 40;

/// Versi skema basis data (`PRAGMA user_version`). Versi 2 menambah
/// `items.background`; profil versi 1 diperbarui saat dibuka (lihat
/// [`upgrade`]).
const SCHEMA_VERSION: i32 = 2;

const SCHEMA: &str = "
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
CREATE TABLE items (
    id         INTEGER PRIMARY KEY,           -- id lokal (Item::id)
    uid        TEXT NOT NULL UNIQUE,          -- UUID v7, sama di semua perangkat
    kind       TEXT NOT NULL CHECK (kind IN ('song', 'verse')),
    title      TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL,              -- milidetik sejak 1970 (UTC)
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,                       -- terisi = tanda hapus
    background TEXT                           -- gambar latar sendiri; NULL = latar umum
);
CREATE TABLE service (
    position INTEGER PRIMARY KEY,
    item_id  INTEGER NOT NULL REFERENCES items(id)
);
";

/// Ringkasan satu profil di perangkat ini, untuk daftar pilihan profil.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// UUID v7, sekaligus nama foldernya.
    pub id: String,
    pub name: String,
    /// Milidetik sejak 1970 (UTC).
    pub created_at: i64,
    /// Terakhir dibuka di perangkat ini (milidetik sejak 1970, UTC).
    pub opened_at: i64,
    /// Jumlah materi (tanpa yang dihapus).
    pub items: usize,
}

/// Hasil [`Store::open`].
#[derive(Debug)]
pub struct Opened {
    pub store: Store,
    pub library: Library,
    /// `true` bila profil baru dibuat tanpa data lama (bukan hasil pemindahan
    /// `library.json`).
    pub fresh: bool,
}

/// Basis data satu profil yang sedang dibuka.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    data_dir: PathBuf,
    path: PathBuf,
    id: String,
    name: String,
    /// Keadaan yang terakhir tersimpan, untuk menulis hanya yang berubah.
    saved: RefCell<Snapshot>,
}

#[derive(Debug, Default)]
struct Snapshot {
    /// id lokal → sidik isi materi.
    items: HashMap<u64, u64>,
    service: Vec<u64>,
    background: usize,
    background_image: Option<String>,
    monitor: usize,
    ready: bool,
}

impl Snapshot {
    /// `items` boleh dikosongkan bila pemanggil sudah menghitung sidiknya.
    fn of(library: &Library, with_items: bool) -> Self {
        let items = if with_items {
            library
                .items
                .iter()
                .map(|item| (item.id, fingerprint(item)))
                .collect()
        } else {
            HashMap::new()
        };
        Self {
            items,
            service: library.service.clone(),
            background: library.background,
            background_image: library.background_image.clone(),
            monitor: library.monitor,
            ready: true,
        }
    }
}

/// Daftar profil yang siap dipakai di folder data, urut dari yang paling lama.
/// Folder profil yang pembuatannya tidak selesai diabaikan. Profil yang rusak
/// atau dibuat Lentera versi lebih baru menghasilkan galat, supaya data lama
/// tidak dipindahkan dua kali ke profil baru.
pub fn profiles(data_dir: &Path) -> Result<Vec<Profile>, String> {
    let root = data_dir.join(PROFILES_DIR);
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Folder profil tidak bisa dibaca.\n{e}")),
    };
    let mut list = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Folder profil tidak bisa dibaca.\n{e}"))?;
        let path = entry.path().join(PROFILE_DB);
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let conn = connect(&path)?;
        if let Some(profile) = read_profile(&conn, &id, &path)? {
            list.push(profile);
        }
    }
    list.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
    Ok(list)
}

/// Mengganti nama profil yang sedang tidak dibuka. Untuk profil yang sedang
/// dibuka, pakai [`Store::rename`].
pub fn rename_profile(data_dir: &Path, id: &str, name: &str) -> Result<(), String> {
    let name = check_name(data_dir, name, Some(id))?;
    let path = profile_db(data_dir, id)?;
    let conn = connect(&path)?;
    read_profile(&conn, id, &path)?.ok_or_else(|| format!("Profil {id} tidak ditemukan."))?;
    set_meta(&conn, "name", &name).map_err(|e| db_error(&path, e))
}

/// Memindahkan profil ke [`DELETED_PROFILES_DIR`]. Profil terakhir tidak bisa
/// dihapus. Pemanggil tidak boleh sedang membuka profil ini.
pub fn remove_profile(data_dir: &Path, id: &str) -> Result<(), String> {
    let path = profile_db(data_dir, id)?;
    let list = profiles(data_dir)?;
    if !list.iter().any(|p| p.id == id) {
        return Err(format!("Profil {id} tidak ditemukan."));
    }
    if list.len() < 2 {
        return Err("Profil terakhir tidak bisa dihapus.".into());
    }
    let trash = data_dir.join(DELETED_PROFILES_DIR);
    fs::create_dir_all(&trash).map_err(|e| format!("Profil tidak bisa dihapus.\n{e}"))?;
    let mut target = trash.join(id);
    if target.exists() {
        target = trash.join(format!("{id}-{}", now_ms()));
    }
    let folder = path.parent().unwrap_or(&path);
    fs::rename(folder, &target).map_err(|e| format!("Profil tidak bisa dihapus.\n{e}"))
}

impl Store {
    /// Membuka profil yang terakhir dibuka di folder data. Bila belum ada
    /// profil, dibuat [`DEFAULT_PROFILE_NAME`]: isinya dipindahkan dari
    /// `library.json` lama bila ada, atau berisi materi contoh. `library.json`
    /// tidak diubah.
    ///
    /// Pemanggil harus sudah memegang [`DataLock`](crate::DataLock) folder ini.
    pub fn open(data_dir: &Path) -> Result<Opened, String> {
        let last = profiles(data_dir)?
            .into_iter()
            .max_by_key(|p| (p.opened_at, p.created_at));
        if let Some(profile) = last {
            return Self::open_profile(data_dir, &profile.id);
        }
        let legacy = data_dir.join(LIBRARY_FILE);
        let fresh = !legacy.exists();
        // Galat di sini (berkas lama rusak) menghentikan pembukaan sebelum
        // profil dibuat; berkas asli tetap utuh.
        let library = Library::load(&legacy)?;
        let store = Self::create(data_dir, DEFAULT_PROFILE_NAME)?;
        store.save(&library)?;
        Ok(Opened {
            store,
            library,
            fresh,
        })
    }

    /// Membuka profil yang sudah ada berdasarkan id-nya, lalu mencatatnya
    /// sebagai profil terakhir dibuka.
    pub fn open_profile(data_dir: &Path, id: &str) -> Result<Opened, String> {
        let path = profile_db(data_dir, id)?;
        let conn = connect(&path)?;
        let profile = read_profile(&conn, id, &path)?
            .ok_or_else(|| format!("Profil {id} belum selesai dibuat."))?;
        upgrade(&conn, &path)?;
        let store = Self {
            conn,
            data_dir: data_dir.into(),
            path,
            id: profile.id,
            name: profile.name,
            saved: RefCell::default(),
        };
        let library = store.load()?;
        *store.saved.borrow_mut() = Snapshot::of(&library, true);
        // Cadangan isi profil saat berhasil dibuka. Aman disalin karena tidak
        // ada transaksi yang berjalan dan folder data sedang dikunci. Gagal
        // membuat cadangan atau mencatat waktu buka tidak menghalangi pembukaan.
        let _ = backup(&store.path);
        let _ = set_meta(&store.conn, "opened_at", &now_ms().to_string());
        Ok(Opened {
            store,
            library,
            fresh: false,
        })
    }

    /// Membuat profil baru tanpa materi dan langsung membukanya. Warna latar
    /// dan monitor diambil dari `like` (biasanya pustaka yang sedang dibuka).
    /// Gambar latar tidak ikut, karena berkasnya ada di folder profil lain;
    /// pemanggil bisa menyalinnya dengan [`crate::background::copy`].
    pub fn create_profile(data_dir: &Path, name: &str, like: &Library) -> Result<Opened, String> {
        let name = check_name(data_dir, name, None)?;
        let store = Self::create(data_dir, &name)?;
        let library = Library {
            background: like.background,
            monitor: like.monitor,
            ..Library::blank()
        };
        if let Err(e) = store.save(&library) {
            // Profil yang belum tersimpan tidak terdaftar; bersihkan foldernya.
            let folder = store.path.parent().map(Path::to_path_buf);
            drop(store);
            if let Some(folder) = folder {
                let _ = fs::remove_dir_all(folder);
            }
            return Err(e);
        }
        Ok(Opened {
            store,
            library,
            fresh: false,
        })
    }

    /// Membuat basis data profil. Profil baru terdaftar di [`profiles`]
    /// setelah [`Store::save`] pertama berhasil.
    fn create(data_dir: &Path, name: &str) -> Result<Self, String> {
        let id = Uuid::now_v7().to_string();
        let dir = data_dir.join(PROFILES_DIR).join(&id);
        fs::create_dir_all(&dir).map_err(|e| format!("Folder profil tidak bisa dibuat.\n{e}"))?;
        let path = dir.join(PROFILE_DB);
        let mut conn = Connection::open(&path).map_err(|e| db_error(&path, e))?;
        let created_at = now_ms();
        let tx = conn.transaction().map_err(|e| db_error(&path, e))?;
        tx.execute_batch(SCHEMA)
            .and_then(|()| tx.pragma_update(None, "user_version", SCHEMA_VERSION))
            .and_then(|()| set_meta(&tx, "name", name))
            .and_then(|()| set_meta(&tx, "created_at", &created_at.to_string()))
            .and_then(|()| set_meta(&tx, "opened_at", &created_at.to_string()))
            .map_err(|e| db_error(&path, e))?;
        tx.commit().map_err(|e| db_error(&path, e))?;
        configure(&conn).map_err(|e| db_error(&path, e))?;
        Ok(Self {
            conn,
            data_dir: data_dir.into(),
            path,
            id,
            name: name.into(),
            saved: RefCell::new(Snapshot::default()),
        })
    }

    /// Id profil yang sedang dibuka.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Nama profil yang sedang dibuka.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Mengganti nama profil yang sedang dibuka.
    pub fn rename(&mut self, name: &str) -> Result<(), String> {
        let name = check_name(&self.data_dir, name, Some(&self.id))?;
        set_meta(&self.conn, "name", &name).map_err(|e| db_error(&self.path, e))?;
        self.name = name;
        Ok(())
    }

    /// Lokasi berkas basis data profil.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Membaca ulang pustaka dari basis data.
    pub fn load(&self) -> Result<Library, String> {
        let err = |e| db_error(&self.path, e);
        let mut items = Vec::new();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, title, content, background FROM items
                 WHERE deleted_at IS NULL ORDER BY id",
            )
            .map_err(err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(err)?;
        for row in rows {
            let (id, kind, title, content, background) = row.map_err(err)?;
            items.push(crate::Item {
                id: to_id(id)?,
                kind: kind_from(&kind)?,
                title,
                content,
                // Nama yang tidak sah diabaikan, seperti latar umum.
                background: background.filter(|name| crate::background::valid_name(name)),
            });
        }
        // Termasuk tanda hapus, supaya id lokal tidak pernah dipakai ulang.
        let max_id: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM items", [], |row| {
                row.get(0)
            })
            .map_err(err)?;
        let mut stmt = self
            .conn
            .prepare("SELECT item_id FROM service ORDER BY position")
            .map_err(err)?;
        let service = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(err)?
            .map(|id| id.map_err(err).and_then(to_id))
            .collect::<Result<Vec<_>, _>>()?;
        let number = |key| -> Result<usize, String> {
            Ok(get_meta(&self.conn, key)
                .map_err(err)?
                .and_then(|value| value.parse().ok())
                .unwrap_or(0))
        };
        let library = Library {
            version: 1,
            next_id: to_id(max_id)? + 1,
            items,
            service,
            background: number("background")?,
            // Nama yang tidak sah diabaikan, seperti latar yang belum dipilih.
            background_image: get_meta(&self.conn, "background_image")
                .map_err(err)?
                .filter(|name| crate::background::valid_name(name)),
            monitor: number("monitor")?,
            scratch: Vec::new(),
        };
        library.validate()?;
        Ok(library)
    }

    /// Menyimpan pustaka dalam satu transaksi. Hanya materi yang berubah sejak
    /// penyimpanan terakhir yang ditulis, dan hanya materi itu yang waktu
    /// ubahnya diperbarui.
    pub fn save(&self, library: &Library) -> Result<(), String> {
        library.validate()?;
        let err = |e| db_error(&self.path, e);
        let mut saved = self.saved.borrow_mut();
        let now = now_ms();
        let mut items = HashMap::with_capacity(library.items.len());
        let tx = self.conn.unchecked_transaction().map_err(err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO items
                         (id, uid, kind, title, content, background, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                )
                .map_err(err)?;
            let mut update = tx
                .prepare(
                    "UPDATE items SET kind = ?2, title = ?3, content = ?4, background = ?5,
                         updated_at = ?6
                     WHERE id = ?1",
                )
                .map_err(err)?;
            for item in &library.items {
                let print = fingerprint(item);
                let id = to_sql_id(item.id)?;
                let kind = kind_name(item.kind);
                let written = match saved.items.get(&item.id) {
                    None => insert.execute(params![
                        id,
                        Uuid::now_v7().to_string(),
                        kind,
                        item.title,
                        item.content,
                        item.background,
                        now
                    ]),
                    Some(old) if *old != print => update.execute(params![
                        id,
                        kind,
                        item.title,
                        item.content,
                        item.background,
                        now
                    ]),
                    Some(_) => Ok(0),
                };
                written.map_err(err)?;
                items.insert(item.id, print);
            }
            let mut remove = tx
                .prepare(
                    "UPDATE items SET title = '', content = '', updated_at = ?2, deleted_at = ?2
                     WHERE id = ?1 AND deleted_at IS NULL",
                )
                .map_err(err)?;
            for id in saved.items.keys().filter(|id| !items.contains_key(id)) {
                remove.execute(params![to_sql_id(*id)?, now]).map_err(err)?;
            }
            if library.service != saved.service {
                tx.execute("DELETE FROM service", []).map_err(err)?;
                let mut add = tx
                    .prepare("INSERT INTO service (position, item_id) VALUES (?1, ?2)")
                    .map_err(err)?;
                for (position, id) in library.service.iter().enumerate() {
                    add.execute(params![position as i64, to_sql_id(*id)?])
                        .map_err(err)?;
                }
            }
            if library.background != saved.background || !saved.ready {
                set_meta(&tx, "background", &library.background.to_string()).map_err(err)?;
            }
            if library.background_image != saved.background_image || !saved.ready {
                let name = library.background_image.as_deref().unwrap_or_default();
                set_meta(&tx, "background_image", name).map_err(err)?;
            }
            if library.monitor != saved.monitor || !saved.ready {
                set_meta(&tx, "monitor", &library.monitor.to_string()).map_err(err)?;
            }
            if !saved.ready {
                set_meta(&tx, "ready", "1").map_err(err)?;
            }
        }
        tx.commit().map_err(|e| format!("Gagal menyimpan: {e}"))?;
        *saved = Snapshot {
            items,
            ..Snapshot::of(library, false)
        };
        Ok(())
    }
}

/// Menyalin basis data ke `library.db.bak` lewat berkas sementara, supaya
/// cadangan lama tidak tertimpa salinan yang setengah jadi.
fn backup(path: &Path) -> std::io::Result<()> {
    let temp = path.with_extension("db.bak.tmp");
    fs::copy(path, &temp)?;
    fs::rename(&temp, path.with_extension("db.bak"))
}

/// Membuka basis data profil yang sudah ada (tidak membuat berkas baru).
fn connect(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| db_error(path, e))?;
    configure(&conn).map_err(|e| db_error(path, e))?;
    Ok(conn)
}

/// Mode jurnal bawaan (rollback) dipertahankan karena juga aman di flashdisk
/// dan folder jaringan; `synchronous = FULL` (2) menjaga data saat listrik padam.
fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "synchronous", 2)
}

/// Memperbarui skema profil lama ke [`SCHEMA_VERSION`] dalam satu transaksi.
/// Profil yang sudah diperbarui tidak bisa dibuka Lentera versi lama, yang
/// menolaknya dengan pesan "dibuat oleh Lentera versi yang lebih baru".
fn upgrade(conn: &Connection, path: &Path) -> Result<(), String> {
    let err = |e| db_error(path, e);
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(err)?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    if version < 2 {
        // Gambar latar per materi.
        tx.execute_batch("ALTER TABLE items ADD COLUMN background TEXT;")
            .map_err(err)?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(err)?;
    tx.commit().map_err(err)
}

/// Membaca identitas profil. `None` berarti pembuatannya belum selesai.
fn read_profile(conn: &Connection, id: &str, path: &Path) -> Result<Option<Profile>, String> {
    let err = |e| db_error(path, e);
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(err)?;
    if version == 0 {
        return Ok(None);
    }
    if version > SCHEMA_VERSION {
        return Err(format!(
            "Profil di {} dibuat oleh Lentera versi yang lebih baru. Perbarui Lentera.",
            path.display()
        ));
    }
    if get_meta(conn, "ready").map_err(err)?.as_deref() != Some("1") {
        return Ok(None);
    }
    let number = |key| -> Result<i64, String> {
        Ok(get_meta(conn, key)
            .map_err(err)?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0))
    };
    let created_at = number("created_at")?;
    let items: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM items WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(err)?;
    Ok(Some(Profile {
        id: id.into(),
        name: get_meta(conn, "name").map_err(err)?.unwrap_or_default(),
        created_at,
        opened_at: number("opened_at")?.max(created_at),
        items: items.max(0) as usize,
    }))
}

/// Lokasi basis data profil. `id` harus nama satu folder, bukan jalur.
fn profile_db(data_dir: &Path, id: &str) -> Result<PathBuf, String> {
    let plain = !id.is_empty()
        && !id.starts_with('.')
        && Path::new(id).file_name().and_then(|n| n.to_str()) == Some(id);
    if !plain {
        return Err(format!("Id profil tidak valid: {id}"));
    }
    Ok(data_dir.join(PROFILES_DIR).join(id).join(PROFILE_DB))
}

/// Merapikan dan memeriksa nama profil: tidak kosong, tidak terlalu panjang,
/// dan belum dipakai profil lain (huruf besar/kecil dianggap sama).
fn check_name(data_dir: &Path, name: &str, except: Option<&str>) -> Result<String, String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("Nama profil belum diisi.".into());
    }
    if name.chars().count() > PROFILE_NAME_MAX {
        return Err(format!(
            "Nama profil paling panjang {PROFILE_NAME_MAX} karakter."
        ));
    }
    let lower = name.to_lowercase();
    let taken = profiles(data_dir)?
        .iter()
        .any(|p| Some(p.id.as_str()) != except && p.name.to_lowercase() == lower);
    if taken {
        return Err(format!("Sudah ada profil bernama \"{name}\"."));
    }
    Ok(name)
}

fn get_meta(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .optional()
}

fn set_meta(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [key, value],
    )
    .map(|_| ())
}

fn fingerprint(item: &crate::Item) -> u64 {
    let mut hasher = DefaultHasher::new();
    item.kind.hash(&mut hasher);
    item.title.hash(&mut hasher);
    item.content.hash(&mut hasher);
    item.background.hash(&mut hasher);
    hasher.finish()
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Song => "song",
        Kind::Verse => "verse",
    }
}

fn kind_from(name: &str) -> Result<Kind, String> {
    match name {
        "song" => Ok(Kind::Song),
        "verse" => Ok(Kind::Verse),
        _ => Err(format!("Jenis materi tidak dikenal: {name}")),
    }
}

fn to_id(value: i64) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("ID materi tidak valid: {value}"))
}

fn to_sql_id(value: u64) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| format!("ID materi terlalu besar: {value}"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn db_error(path: &Path, e: rusqlite::Error) -> String {
    format!("Basis data pustaka {} bermasalah.\n{e}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Item;

    /// Folder data sementara yang dihapus saat selesai.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("lentera-store-{}-{name}", std::process::id()));
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

    fn ids(data_dir: &Path) -> Vec<String> {
        profiles(data_dir)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect()
    }

    fn pause() {
        // Waktu dicatat dalam milidetik; beri jarak supaya urutannya pasti.
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    fn row(store: &Store, id: u64) -> (String, String, String, i64, i64, Option<i64>) {
        store
            .conn
            .query_row(
                "SELECT uid, title, content, created_at, updated_at, deleted_at
                 FROM items WHERE id = ?1",
                [id as i64],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .unwrap()
    }

    #[test]
    fn first_open_creates_one_profile_with_sample_library() {
        let dir = TempDir::new("fresh");
        let opened = Store::open(&dir.0).unwrap();
        assert!(opened.fresh);
        assert_eq!(opened.library, Library::default());
        assert_eq!(opened.store.name(), DEFAULT_PROFILE_NAME);
        let id = opened.store.id().to_owned();
        assert_eq!(Uuid::parse_str(&id).unwrap().get_version_num(), 7);
        drop(opened);

        let again = Store::open(&dir.0).unwrap();
        assert!(!again.fresh);
        assert_eq!(again.store.id(), id);
        assert_eq!(again.library, Library::default());
        assert_eq!(profiles(&dir.0).unwrap().len(), 1);
    }

    #[test]
    fn old_library_json_moves_into_first_profile_and_stays_untouched() {
        let dir = TempDir::new("migrate");
        let mut old = Library::default();
        old.add(Kind::Song, "Lagu ✓".into(), "Satu\n\nDua".into());
        old.remove(2);
        old.service = vec![3, 1, 3];
        old.background = 2;
        old.monitor = 1;
        let json = serde_json::to_vec_pretty(&old).unwrap();
        fs::write(dir.0.join(LIBRARY_FILE), &json).unwrap();

        let opened = Store::open(&dir.0).unwrap();
        assert!(!opened.fresh);
        assert_eq!(opened.library, old);
        drop(opened);
        assert_eq!(fs::read(dir.0.join(LIBRARY_FILE)).unwrap(), json);

        // Dibuka lagi: dibaca dari profil, tidak dipindahkan kedua kali.
        let again = Store::open(&dir.0).unwrap();
        assert_eq!(again.library.items, old.items);
        assert_eq!(again.library.service, old.service);
        assert_eq!((again.library.background, again.library.monitor), (2, 1));
        assert_eq!(profiles(&dir.0).unwrap().len(), 1);
    }

    #[test]
    fn corrupt_library_json_stops_before_any_profile_is_created() {
        let dir = TempDir::new("corrupt");
        fs::write(dir.0.join(LIBRARY_FILE), b"broken").unwrap();
        assert!(Store::open(&dir.0).is_err());
        assert!(profiles(&dir.0).unwrap().is_empty());
        assert_eq!(fs::read(dir.0.join(LIBRARY_FILE)).unwrap(), b"broken");
    }

    #[test]
    fn save_writes_changes_and_keeps_deletes_as_tombstones() {
        let dir = TempDir::new("save");
        let Opened {
            store, mut library, ..
        } = Store::open(&dir.0).unwrap();
        let (uid1, _, _, created1, updated1, _) = row(&store, 1);
        let (_, _, _, _, updated2, _) = row(&store, 2);
        std::thread::sleep(std::time::Duration::from_millis(5));

        assert!(library.update(1, "Judul baru", "A\n\nB"));
        let added = library.add(Kind::Verse, "Ayat".into(), "Teks".into());
        library.remove(2);
        library.service = vec![added, 1];
        library.background = 1;
        store.save(&library).unwrap();

        let (uid, title, content, created, updated, deleted) = row(&store, 1);
        assert_eq!((uid, created), (uid1.clone(), created1));
        assert_eq!((title.as_str(), content.as_str()), ("Judul baru", "A\n\nB"));
        assert!(updated > updated1 && deleted.is_none());
        let (uid2, title, content, _, updated, deleted) = row(&store, 2);
        assert_eq!((title.as_str(), content.as_str()), ("", ""));
        assert!(deleted.is_some() && updated > updated2);
        assert!(Uuid::parse_str(&row(&store, added).0).is_ok());

        // Menyimpan tanpa perubahan tidak menyentuh waktu ubah.
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.save(&library).unwrap();
        assert_eq!(row(&store, 1).4, updated);
        drop(store);

        let Opened {
            store,
            library: reopened,
            ..
        } = Store::open(&dir.0).unwrap();
        assert_eq!(reopened.items, library.items);
        assert_eq!(reopened.service, vec![added, 1]);
        assert_eq!(reopened.background, 1);
        assert_eq!(row(&store, 2).0, uid2);
        // Cadangan berisi keadaan saat profil dibuka.
        let bak = Connection::open(store.path().with_extension("db.bak")).unwrap();
        let titles: String = bak
            .query_row(
                "SELECT group_concat(title, '|') FROM (SELECT title FROM items ORDER BY id)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(titles, "Judul baru||Ayat");
        drop(bak);
        // id materi yang dihapus tidak dipakai ulang.
        assert!(reopened.next_id > added);
        let mut reopened = reopened;
        let next = reopened.add(Kind::Song, "Baru".into(), String::new());
        assert!(next > added && next != 2);
        store.save(&reopened).unwrap();
        assert_ne!(row(&store, next).0, uid1);
        // Cadangan tidak ikut berubah saat menyimpan.
        let bak = Connection::open(store.path().with_extension("db.bak")).unwrap();
        let count: i64 = bak
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn unfinished_profiles_are_ignored_and_newer_ones_refuse_to_open() {
        let dir = TempDir::new("versions");
        // Pembuatan profil terputus sebelum penyimpanan pertama.
        drop(Store::create(&dir.0, "Terputus").unwrap());
        assert!(profiles(&dir.0).unwrap().is_empty());
        let opened = Store::open(&dir.0).unwrap();
        assert_eq!(ids(&dir.0), vec![opened.store.id().to_owned()]);
        opened
            .store
            .conn
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(opened);
        // Jangan sampai data lama dipindahkan lagi ke profil baru.
        fs::write(dir.0.join(LIBRARY_FILE), b"{}").unwrap();
        assert!(
            Store::open(&dir.0)
                .unwrap_err()
                .contains("versi yang lebih baru")
        );
    }

    #[test]
    fn invalid_library_is_not_saved() {
        let dir = TempDir::new("invalid");
        let Opened {
            store, mut library, ..
        } = Store::open(&dir.0).unwrap();
        library.service.push(99);
        assert!(store.save(&library).is_err());
        library.service.pop();
        library.items.push(Item {
            id: 1,
            kind: Kind::Song,
            title: "Ganda".into(),
            content: String::new(),
            background: None,
        });
        assert!(store.save(&library).is_err());
        assert_eq!(store.load().unwrap(), Library::default());
    }

    #[test]
    fn new_profile_starts_empty_and_keeps_screen_settings() {
        let dir = TempDir::new("create");
        let first = Store::open(&dir.0).unwrap();
        let like = Library {
            background: 2,
            monitor: 1,
            ..first.library.clone()
        };
        let made = Store::create_profile(&dir.0, "  Komisi   Pemuda ", &like).unwrap();
        assert_eq!(made.store.name(), "Komisi Pemuda");
        assert!(made.library.items.is_empty() && made.library.service.is_empty());
        assert_eq!((made.library.background, made.library.monitor), (2, 1));
        let list = profiles(&dir.0).unwrap();
        assert_eq!(list.len(), 2);
        let counts: Vec<(String, usize)> = list.into_iter().map(|p| (p.name, p.items)).collect();
        assert!(counts.contains(&(DEFAULT_PROFILE_NAME.to_owned(), 2)));
        assert!(counts.contains(&("Komisi Pemuda".to_owned(), 0)));
        drop(made);
        let again = Store::open_profile(&dir.0, &ids(&dir.0)[1]).unwrap();
        assert_eq!(
            again.library,
            Library {
                background: 2,
                monitor: 1,
                ..Library::blank()
            }
        );
    }

    #[test]
    fn item_backgrounds_are_saved_and_old_profiles_are_upgraded() {
        let dir = TempDir::new("latar-materi");
        let Opened {
            store, mut library, ..
        } = Store::open(&dir.0).unwrap();
        assert!(library.set_item_background(1, Some("Salib.jpg".into())));
        store.save(&library).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.items[0].background.as_deref(), Some("Salib.jpg"));
        assert_eq!(loaded.items[1].background, None);
        // Hanya latarnya yang berubah: tetap tersimpan.
        library.set_item_background(1, None);
        store.save(&library).unwrap();
        assert_eq!(store.load().unwrap(), library);
        // Nama yang menunjuk ke luar folder latar tidak pernah dipakai.
        store
            .conn
            .execute(
                "UPDATE items SET background = '../library.db' WHERE id = 2",
                [],
            )
            .unwrap();
        assert_eq!(store.load().unwrap().items[1].background, None);

        // Profil skema 1 (Lentera 0.3–0.4) diperbarui saat dibuka; isinya tetap.
        let id = store.id().to_owned();
        store
            .conn
            .execute_batch("ALTER TABLE items DROP COLUMN background; PRAGMA user_version = 1;")
            .unwrap();
        drop(store);
        assert_eq!(ids(&dir.0), vec![id.clone()]);
        let Opened {
            store, mut library, ..
        } = Store::open_profile(&dir.0, &id).unwrap();
        let version: i32 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(library.items.len(), 2);
        assert!(library.items.iter().all(|i| i.background.is_none()));
        library.set_item_background(2, Some("Salib.jpg".into()));
        store.save(&library).unwrap();
        assert_eq!(
            store.load().unwrap().items[1].background.as_deref(),
            Some("Salib.jpg")
        );
    }

    #[test]
    fn background_image_is_saved_and_bad_names_are_ignored() {
        let dir = TempDir::new("latar");
        let opened = Store::open(&dir.0).unwrap();
        let mut library = opened.library.clone();
        library.background_image = Some("Salib senja.jpg".into());
        opened.store.save(&library).unwrap();
        let loaded = opened.store.load().unwrap();
        assert_eq!(loaded.background_image.as_deref(), Some("Salib senja.jpg"));
        library.background_image = None;
        opened.store.save(&library).unwrap();
        assert_eq!(opened.store.load().unwrap().background_image, None);
        // Nama yang menunjuk ke luar folder latar tidak pernah dipakai.
        set_meta(&opened.store.conn, "background_image", "../library.db").unwrap();
        assert_eq!(opened.store.load().unwrap().background_image, None);
        // Profil baru tidak ikut membawa gambar; berkasnya disalin pemanggil.
        library.background_image = Some("Salib senja.jpg".into());
        let made = Store::create_profile(&dir.0, "Pemuda", &library).unwrap();
        assert_eq!(made.library.background_image, None);
    }

    #[test]
    fn profile_names_must_be_filled_short_and_unique() {
        let dir = TempDir::new("names");
        let mut first = Store::open(&dir.0).unwrap();
        let like = first.library.clone();
        assert!(
            Store::create_profile(&dir.0, " \t ", &like)
                .unwrap_err()
                .contains("belum diisi")
        );
        let long = "x".repeat(PROFILE_NAME_MAX + 1);
        assert!(
            Store::create_profile(&dir.0, &long, &like)
                .unwrap_err()
                .contains("paling panjang")
        );
        assert!(
            Store::create_profile(&dir.0, "profil UTAMA", &like)
                .unwrap_err()
                .contains("Sudah ada")
        );
        assert_eq!(ids(&dir.0).len(), 1);
        // Nama sendiri boleh dipakai lagi dengan huruf berbeda.
        first.store.rename("PROFIL utama ✓").unwrap();
        assert_eq!(first.store.name(), "PROFIL utama ✓");
        let other = Store::create_profile(&dir.0, "Ibadah Umum", &like).unwrap();
        let other_id = other.store.id().to_owned();
        drop(other);
        assert!(
            first
                .store
                .rename("ibadah umum")
                .unwrap_err()
                .contains("Sudah ada")
        );
        assert!(rename_profile(&dir.0, &other_id, "profil utama ✓").is_err());
        rename_profile(&dir.0, &other_id, "Ibadah Pagi").unwrap();
        let names: Vec<String> = profiles(&dir.0)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["PROFIL utama ✓", "Ibadah Pagi"]);
        assert!(rename_profile(&dir.0, "../luar", "x").is_err());
    }

    #[test]
    fn open_returns_the_profile_opened_last() {
        let dir = TempDir::new("last");
        let first = Store::open(&dir.0).unwrap();
        let first_id = first.store.id().to_owned();
        pause();
        let second = Store::create_profile(&dir.0, "Kedua", &first.library).unwrap();
        let second_id = second.store.id().to_owned();
        drop((first, second));
        assert_eq!(Store::open(&dir.0).unwrap().store.id(), second_id);
        pause();
        drop(Store::open_profile(&dir.0, &first_id).unwrap());
        assert_eq!(Store::open(&dir.0).unwrap().store.id(), first_id);
    }

    #[test]
    fn removed_profiles_move_aside_and_the_last_one_stays() {
        let dir = TempDir::new("remove");
        let first = Store::open(&dir.0).unwrap();
        let first_id = first.store.id().to_owned();
        let second = Store::create_profile(&dir.0, "Kedua", &first.library).unwrap();
        let second_id = second.store.id().to_owned();
        drop((first, second));
        remove_profile(&dir.0, &second_id).unwrap();
        assert_eq!(ids(&dir.0), vec![first_id.clone()]);
        assert!(
            dir.0
                .join(DELETED_PROFILES_DIR)
                .join(&second_id)
                .join(PROFILE_DB)
                .is_file()
        );
        assert!(
            remove_profile(&dir.0, &first_id)
                .unwrap_err()
                .contains("terakhir")
        );
        assert!(remove_profile(&dir.0, &second_id).is_err());
        assert!(remove_profile(&dir.0, "..").is_err());
        assert_eq!(Store::open(&dir.0).unwrap().store.id(), first_id);
    }
}
