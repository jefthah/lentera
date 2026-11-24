//! Catatan sinkron di basis data profil (`library.db`).
//!
//! Selain kunci `meta` yang ditulis [`Store`](crate::Store) saat nama,
//! susunan, atau latar berubah, sinkron memakai:
//!
//! - tabel `synced`: per materi, `updated_at` terakhir yang sudah sama dengan
//!   server. Materi yang `updated_at`-nya berbeda (atau belum tercatat) belum
//!   terkirim. Perubahan yang dibuat Lentera versi lama juga terdeteksi,
//!   karena versi itu tetap memperbarui `updated_at`;
//! - kunci `meta`: akun pemilik sambungan, cursor server, dan waktu nama /
//!   susunan / latar terakhir yang sudah sama dengan server.
//!
//! Semuanya tambahan pada skema 2, jadi profil tetap bisa dibuka Lentera 0.5
//! dan 0.6.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, params};

use super::{RemoteItem, RemoteProfile, RemoteService, RemoteSettings};
use crate::store::{
    self, BACKGROUND_AT, NAME_AT, PROFILE_DB, PROFILES_DIR, SCHEMA, SCHEMA_VERSION, SERVICE_AT,
};

/// Akun yang menyambungkan profil ini ke server; kosong bila belum.
const ACCOUNT: &str = "sync_account";
/// Revisi server terakhir yang sudah diterima.
const CURSOR: &str = "sync_cursor";
/// Kosong, `deleted` (dihapus pemiliknya di server), atau `removed` (akun
/// ini tidak lagi punya akses). Profil yang tidak kosong tidak disinkron lagi.
const STATE: &str = "sync_state";
const NAME_SYNCED: &str = "name_synced";
const SERVICE_SYNCED: &str = "service_synced";
const BACKGROUND_SYNCED: &str = "background_synced";

const TABLES: &str = "
CREATE TABLE IF NOT EXISTS synced (
    uid        TEXT PRIMARY KEY,
    updated_at INTEGER NOT NULL
) WITHOUT ROWID;
";

/// Jumlah warna latar umum yang dikenal aplikasi (Gelap, Biru malam, Terang).
/// Nilai lain dari server diabaikan.
const BACKGROUNDS: i64 = 3;

/// Keadaan sinkron satu profil.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Info {
    pub name: String,
    pub created_at: i64,
    pub name_at: i64,
    pub name_synced: i64,
    pub account: String,
    pub cursor: i64,
    pub state: String,
}

/// Materi yang belum terkirim, sebanyak yang muat di satu permintaan.
#[derive(Debug, Default)]
pub(crate) struct Chunk {
    pub items: Vec<RemoteItem>,
    /// Masih ada materi lain yang belum terkirim setelah bagian ini.
    pub more: bool,
    /// Materi yang melewati batas server: (uid, judul).
    pub oversize: Vec<(String, String)>,
}

/// Satu jawaban sinkron yang diterapkan ke basis data.
#[derive(Debug, Default)]
pub(crate) struct Page<'a> {
    /// Materi yang dikirim di permintaan ini.
    pub sent: &'a [RemoteItem],
    /// `updated_at` susunan / latar yang dikirim, bila ikut dikirim.
    pub sent_service: Option<i64>,
    pub sent_settings: Option<i64>,
    pub items: &'a [RemoteItem],
    pub service: Option<&'a RemoteService>,
    pub settings: Option<&'a RemoteSettings>,
    pub cursor: i64,
}

/// Koneksi ke basis data satu profil khusus untuk sinkron. Tidak mencatat
/// waktu buka dan tidak membuat cadangan, tidak seperti
/// [`Store::open_profile`](crate::Store::open_profile).
pub(crate) struct SyncDb {
    conn: Connection,
    path: PathBuf,
}

impl SyncDb {
    pub fn open(data_dir: &Path, id: &str) -> Result<Self, String> {
        let path = store::profile_db(data_dir, id)?;
        if !path.is_file() {
            return Err(format!("Profil {id} tidak ditemukan."));
        }
        let conn = store::connect(&path)?;
        store::read_profile(&conn, id, &path)?
            .ok_or_else(|| format!("Profil {id} belum selesai dibuat."))?;
        store::upgrade(&conn, &path)?;
        conn.execute_batch(TABLES)
            .map_err(|e| store::db_error(&path, e))?;
        Ok(Self { conn, path })
    }

    fn err(&self, e: rusqlite::Error) -> String {
        store::db_error(&self.path, e)
    }

    fn number(&self, key: &str) -> Result<i64, String> {
        Ok(store::get_meta(&self.conn, key)
            .map_err(|e| self.err(e))?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0))
    }

    fn text(&self, key: &str) -> Result<String, String> {
        Ok(store::get_meta(&self.conn, key)
            .map_err(|e| self.err(e))?
            .unwrap_or_default())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        store::set_meta(&self.conn, key, value).map_err(|e| self.err(e))
    }

    pub fn info(&self) -> Result<Info, String> {
        let created_at = self.number("created_at")?;
        Ok(Info {
            name: self.text("name")?,
            created_at,
            name_at: self.number(NAME_AT)?.max(created_at),
            name_synced: self.number(NAME_SYNCED)?,
            account: self.text(ACCOUNT)?,
            cursor: self.number(CURSOR)?,
            state: self.text(STATE)?,
        })
    }

    /// Menyambungkan profil ke akun setelah server menerimanya. Profil yang
    /// baru pertama kali disambungkan mengirim susunan dan latarnya, walau
    /// dibuat sebelum Lentera mencatat waktu perubahannya.
    pub fn link(&self, account: &str, name_at: i64) -> Result<(), String> {
        let first = self.text(ACCOUNT)? != account;
        let now = store::now_ms();
        if first && self.number(SERVICE_AT)? == 0 {
            self.set(SERVICE_AT, &now.to_string())?;
        }
        if first && self.number(BACKGROUND_AT)? == 0 {
            self.set(BACKGROUND_AT, &now.to_string())?;
        }
        self.set(ACCOUNT, account)?;
        self.set(NAME_SYNCED, &name_at.to_string())?;
        self.set(STATE, "")
    }

    /// Menandai nama profil sudah sama dengan server tanpa mengubahnya.
    pub fn name_synced(&self, at: i64) -> Result<(), String> {
        self.set(NAME_SYNCED, &at.to_string())
    }

    /// Memakai nama dari server. Nama yang sudah dipakai profil lain di
    /// perangkat ini diberi akhiran " (2)", " (3)", dan seterusnya.
    /// Mengembalikan `true` bila nama lokal berubah.
    pub fn set_name(&self, data_dir: &Path, id: &str, name: &str, at: i64) -> Result<bool, String> {
        let name = unique_name(data_dir, id, name)?;
        let changed = self.text("name")? != name;
        self.set("name", &name)?;
        self.set(NAME_AT, &at.to_string())?;
        self.set(NAME_SYNCED, &at.to_string())?;
        Ok(changed)
    }

    pub fn set_state(&self, state: &str) -> Result<(), String> {
        self.set(STATE, state)
    }

    /// Mulai ulang dari nol: server lebih lama daripada perangkat (misalnya
    /// dipulihkan dari cadangan), jadi semua isi lokal dikirim ulang.
    pub fn reset(&self) -> Result<(), String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| self.err(e))?;
        tx.execute("DELETE FROM synced", [])
            .map_err(|e| self.err(e))?;
        for key in [SERVICE_SYNCED, BACKGROUND_SYNCED] {
            store::set_meta(&tx, key, "-1").map_err(|e| self.err(e))?;
        }
        store::set_meta(&tx, CURSOR, "0").map_err(|e| self.err(e))?;
        tx.commit().map_err(|e| self.err(e))
    }

    /// Ada yang belum terkirim: materi, susunan, latar, atau nama.
    pub fn has_changes(&self) -> Result<bool, String> {
        let items: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM items i LEFT JOIN synced s ON s.uid = i.uid
                 WHERE s.updated_at IS NULL OR s.updated_at <> i.updated_at",
                [],
                |row| row.get(0),
            )
            .map_err(|e| self.err(e))?;
        let info = self.info()?;
        Ok(items > 0
            || info.name_at != info.name_synced
            || self.dirty_service()?.is_some()
            || self.dirty_settings()?.is_some())
    }

    /// Materi yang belum terkirim, paling banyak `limit` materi dan sekitar
    /// `max_bytes` byte. Materi di `skip` dilewati.
    pub fn dirty_items(
        &self,
        limit: usize,
        max_bytes: usize,
        skip: &HashSet<String>,
    ) -> Result<Chunk, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT i.uid, i.kind, i.title, i.content, i.created_at, i.updated_at, i.deleted_at
                 FROM items i LEFT JOIN synced s ON s.uid = i.uid
                 WHERE s.updated_at IS NULL OR s.updated_at <> i.updated_at
                 ORDER BY i.id",
            )
            .map_err(|e| self.err(e))?;
        let mut rows = stmt.query([]).map_err(|e| self.err(e))?;
        let mut chunk = Chunk::default();
        let mut bytes = 0;
        while let Some(row) = rows.next().map_err(|e| self.err(e))? {
            let item = RemoteItem {
                id: row.get(0).map_err(|e| self.err(e))?,
                kind: row.get(1).map_err(|e| self.err(e))?,
                title: row.get(2).map_err(|e| self.err(e))?,
                content: row.get(3).map_err(|e| self.err(e))?,
                created_at: row.get(4).map_err(|e| self.err(e))?,
                updated_at: row.get(5).map_err(|e| self.err(e))?,
                deleted_at: row.get(6).map_err(|e| self.err(e))?,
            };
            if skip.contains(&item.id) {
                continue;
            }
            if item.title.chars().count() > super::MAX_TITLE_CHARS
                || item.content.len() > super::MAX_CONTENT_BYTES
            {
                chunk.oversize.push((item.id, item.title));
                continue;
            }
            let size = item.title.len() + item.content.len() + 200;
            if !chunk.items.is_empty() && (chunk.items.len() >= limit || bytes + size > max_bytes) {
                chunk.more = true;
                break;
            }
            bytes += size;
            chunk.items.push(item);
        }
        Ok(chunk)
    }

    /// Susunan ibadah bila belum terkirim. Materi yang sudah dihapus tidak
    /// ikut, karena server hanya menerima susunan berisi materi yang ada.
    pub fn dirty_service(&self) -> Result<Option<RemoteService>, String> {
        let at = self.number(SERVICE_AT)?;
        if at == 0 || at == self.number(SERVICE_SYNCED)? {
            return Ok(None);
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT i.uid FROM service s JOIN items i ON i.id = s.item_id
                 WHERE i.deleted_at IS NULL ORDER BY s.position",
            )
            .map_err(|e| self.err(e))?;
        let items = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| self.err(e))?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|e| self.err(e))?;
        Ok(Some(RemoteService {
            items,
            updated_at: at,
        }))
    }

    /// Warna latar umum bila belum terkirim. Gambar latar tidak ikut: gambar
    /// belum disinkron.
    pub fn dirty_settings(&self) -> Result<Option<RemoteSettings>, String> {
        let at = self.number(BACKGROUND_AT)?;
        if at == 0 || at == self.number(BACKGROUND_SYNCED)? {
            return Ok(None);
        }
        Ok(Some(RemoteSettings {
            background: self.number("background")?,
            updated_at: at,
        }))
    }

    /// Menerapkan satu jawaban server dalam satu transaksi, lalu menyimpan
    /// cursor barunya. Mengembalikan `true` bila isi pustaka berubah.
    ///
    /// Perubahan lokal yang dibuat selama permintaan berjalan (lebih baru
    /// daripada versi dari server) dipertahankan dan dikirim berikutnya.
    pub fn apply(&self, page: &Page) -> Result<bool, String> {
        let err = |e| self.err(e);
        let tx = self.conn.unchecked_transaction().map_err(err)?;
        let mut changed = false;
        {
            // Kiriman yang diterima: tandai terkirim bila materinya belum
            // diubah lagi sejak dikirim.
            let mut mark = tx
                .prepare(
                    "INSERT INTO synced (uid, updated_at)
                     SELECT uid, updated_at FROM items WHERE uid = ?1 AND updated_at = ?2
                     ON CONFLICT (uid) DO UPDATE SET updated_at = excluded.updated_at",
                )
                .map_err(err)?;
            for item in page.sent {
                mark.execute(params![item.id, item.updated_at])
                    .map_err(err)?;
            }
            let mut find = tx
                .prepare(
                    "SELECT i.id, i.updated_at, s.updated_at
                     FROM items i LEFT JOIN synced s ON s.uid = i.uid WHERE i.uid = ?1",
                )
                .map_err(err)?;
            let mut update = tx
                .prepare(
                    "UPDATE items SET kind = ?2, title = ?3, content = ?4, created_at = ?5,
                         updated_at = ?6, deleted_at = ?7
                     WHERE id = ?1",
                )
                .map_err(err)?;
            let mut insert = tx
                .prepare(
                    "INSERT INTO items (id, uid, kind, title, content, created_at, updated_at, deleted_at)
                     VALUES ((SELECT COALESCE(MAX(id), 0) + 1 FROM items), ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(err)?;
            let mut unlist = tx
                .prepare("DELETE FROM service WHERE item_id = ?1")
                .map_err(err)?;
            let mut synced = tx
                .prepare(
                    "INSERT INTO synced (uid, updated_at) VALUES (?1, ?2)
                     ON CONFLICT (uid) DO UPDATE SET updated_at = excluded.updated_at",
                )
                .map_err(err)?;
            for item in page.items {
                let Ok(kind) = store::kind_from(&item.kind) else {
                    continue;
                };
                let kind = store::kind_name(kind);
                let (title, content) = match item.deleted_at {
                    Some(_) => ("", ""),
                    None => (item.title.as_str(), item.content.as_str()),
                };
                let local: Option<(i64, i64, Option<i64>)> = find
                    .query_row([&item.id], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .optional()
                    .map_err(err)?;
                match local {
                    Some((_, at, synced_at)) if synced_at != Some(at) && at > item.updated_at => {
                        // Diubah lagi di sini setelah versi server ini dibuat.
                        continue;
                    }
                    Some((id, ..)) => {
                        update
                            .execute(params![
                                id,
                                kind,
                                title,
                                content,
                                item.created_at,
                                item.updated_at,
                                item.deleted_at
                            ])
                            .map_err(err)?;
                        if item.deleted_at.is_some() {
                            unlist.execute([id]).map_err(err)?;
                        }
                    }
                    None => {
                        insert
                            .execute(params![
                                item.id,
                                kind,
                                title,
                                content,
                                item.created_at,
                                item.updated_at,
                                item.deleted_at
                            ])
                            .map_err(err)?;
                    }
                }
                synced
                    .execute(params![item.id, item.updated_at])
                    .map_err(err)?;
                changed = true;
            }
        }
        changed |= self.apply_service(&tx, page)?;
        changed |= self.apply_settings(&tx, page)?;
        store::set_meta(&tx, CURSOR, &page.cursor.to_string()).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(changed)
    }

    fn apply_service(&self, tx: &Connection, page: &Page) -> Result<bool, String> {
        let err = |e| self.err(e);
        let local_at = meta_number(tx, SERVICE_AT).map_err(err)?;
        if page.sent_service == Some(local_at) {
            store::set_meta(tx, SERVICE_SYNCED, &local_at.to_string()).map_err(err)?;
        }
        let Some(remote) = page.service else {
            return Ok(false);
        };
        let synced = meta_number(tx, SERVICE_SYNCED).map_err(err)?;
        if local_at != 0 && local_at != synced && local_at > remote.updated_at {
            return Ok(false);
        }
        tx.execute("DELETE FROM service", []).map_err(err)?;
        let mut find = tx
            .prepare("SELECT id FROM items WHERE uid = ?1 AND deleted_at IS NULL")
            .map_err(err)?;
        let mut add = tx
            .prepare("INSERT INTO service (position, item_id) VALUES (?1, ?2)")
            .map_err(err)?;
        let mut position = 0_i64;
        for uid in &remote.items {
            let id: Option<i64> = find
                .query_row([uid], |row| row.get(0))
                .optional()
                .map_err(err)?;
            if let Some(id) = id {
                add.execute([position, id]).map_err(err)?;
                position += 1;
            }
        }
        let at = remote.updated_at.to_string();
        store::set_meta(tx, SERVICE_AT, &at).map_err(err)?;
        store::set_meta(tx, SERVICE_SYNCED, &at).map_err(err)?;
        Ok(true)
    }

    fn apply_settings(&self, tx: &Connection, page: &Page) -> Result<bool, String> {
        let err = |e| self.err(e);
        let local_at = meta_number(tx, BACKGROUND_AT).map_err(err)?;
        if page.sent_settings == Some(local_at) {
            store::set_meta(tx, BACKGROUND_SYNCED, &local_at.to_string()).map_err(err)?;
        }
        let Some(remote) = page.settings else {
            return Ok(false);
        };
        let synced = meta_number(tx, BACKGROUND_SYNCED).map_err(err)?;
        if local_at != 0 && local_at != synced && local_at > remote.updated_at {
            return Ok(false);
        }
        if (0..BACKGROUNDS).contains(&remote.background) {
            store::set_meta(tx, "background", &remote.background.to_string()).map_err(err)?;
        }
        let at = remote.updated_at.to_string();
        store::set_meta(tx, BACKGROUND_AT, &at).map_err(err)?;
        store::set_meta(tx, BACKGROUND_SYNCED, &at).map_err(err)?;
        Ok(true)
    }
}

fn meta_number(conn: &Connection, key: &str) -> rusqlite::Result<i64> {
    Ok(store::get_meta(conn, key)?
        .and_then(|value| value.parse().ok())
        .unwrap_or(0))
}

/// Nama profil yang belum dipakai profil lain di perangkat ini.
fn unique_name(data_dir: &Path, id: &str, name: &str) -> Result<String, String> {
    if let Ok(name) = store::check_name(data_dir, name, Some(id)) {
        return Ok(name);
    }
    let base = name.split_whitespace().collect::<Vec<_>>().join(" ");
    for n in 2..1000 {
        let suffix = format!(" ({n})");
        let room = store::PROFILE_NAME_MAX.saturating_sub(suffix.chars().count());
        let candidate: String = base.chars().take(room).collect::<String>() + &suffix;
        if let Ok(name) = store::check_name(data_dir, &candidate, Some(id)) {
            return Ok(name);
        }
    }
    Err(format!("Nama profil \"{name}\" tidak bisa dipakai."))
}

/// Membuat profil kosong di perangkat ini untuk profil yang ada di server,
/// dengan id dan nama dari server. Isinya diisi oleh sinkron berikutnya.
pub(crate) fn create_remote_profile(
    data_dir: &Path,
    account: &str,
    remote: &RemoteProfile,
) -> Result<(), String> {
    let path = store::profile_db(data_dir, &remote.id)?;
    if path.exists() {
        return Err(format!("Profil {} sudah ada.", remote.id));
    }
    let name = unique_name(data_dir, &remote.id, &remote.name)?;
    let dir = data_dir.join(PROFILES_DIR).join(&remote.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Folder profil tidak bisa dibuat.\n{e}"))?;
    let path = dir.join(PROFILE_DB);
    let result = (|| {
        let mut conn = Connection::open(&path)?;
        let tx = conn.transaction()?;
        tx.execute_batch(SCHEMA)?;
        tx.execute_batch(TABLES)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        let name_at = remote.name_updated_at.to_string();
        for (key, value) in [
            ("name", name.as_str()),
            (NAME_AT, &name_at),
            (NAME_SYNCED, &name_at),
            ("created_at", &remote.created_at.to_string()),
            ("opened_at", "0"),
            ("background", "0"),
            ("monitor", "0"),
            (ACCOUNT, account),
            (CURSOR, "0"),
            ("ready", "1"),
        ] {
            store::set_meta(&tx, key, value)?;
        }
        tx.commit()
    })();
    result.map_err(|e| {
        let _ = std::fs::remove_dir_all(&dir);
        store::db_error(&path, e)
    })
}
