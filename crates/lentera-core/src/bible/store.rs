//! `alkitab.db`: terjemahan Alkitab yang sudah diimpor.

use super::Verse;
use super::reference::{END, Reference, parse};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Nama berkas basis data Alkitab di folder data.
pub const BIBLE_DB: &str = "alkitab.db";
/// Batas ayat dalam satu referensi.
pub const MAX_PASSAGE_VERSES: usize = 300;
/// Panjang maksimum singkatan terjemahan.
const CODE_MAX: usize = 12;
/// Panjang maksimum nama terjemahan.
const NAME_MAX: usize = 80;

const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = "
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
CREATE TABLE translations (
    id          INTEGER PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE COLLATE NOCASE, -- singkatan, misalnya TB
    name        TEXT NOT NULL,
    source      TEXT NOT NULL,                       -- nama berkas asal
    verses      INTEGER NOT NULL,
    imported_at INTEGER NOT NULL                     -- milidetik sejak 1970 (UTC)
);
CREATE TABLE verses (
    translation INTEGER NOT NULL REFERENCES translations(id) ON DELETE CASCADE,
    book        INTEGER NOT NULL,                    -- 1-66
    chapter     INTEGER NOT NULL,
    verse       INTEGER NOT NULL,
    last        INTEGER NOT NULL,                    -- ayat gabungan: nomor terakhir
    text        TEXT NOT NULL,
    PRIMARY KEY (translation, book, chapter, verse)
) WITHOUT ROWID;
";

/// Satu terjemahan yang sudah diimpor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translation {
    pub id: i64,
    /// Singkatan, misalnya "TB". Ditulis di judul materi.
    pub code: String,
    pub name: String,
    pub verses: usize,
}

/// Hasil [`Bible::lookup`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lookup {
    /// Referensi yang sudah disesuaikan dengan isi terjemahan.
    pub reference: Reference,
    /// Tulisan baku referensi, misalnya "Yohanes 3:16-18".
    pub label: String,
    /// Jumlah pasal kitab ini di terjemahan.
    pub chapters: u16,
    /// Pasal yang ditampilkan di daftar ayat (pasal pertama referensi).
    pub chapter: u16,
    /// Semua ayat di `chapter`.
    pub chapter_verses: Vec<Verse>,
    /// Ayat yang dirujuk, urut sesuai referensi. Kosong bila hanya nama kitab.
    pub verses: Vec<Verse>,
}

/// Basis data Alkitab yang sedang dibuka.
#[derive(Debug)]
pub struct Bible {
    conn: Connection,
    path: PathBuf,
}

impl Bible {
    /// Membuka `alkitab.db` di folder data, dan membuatnya bila belum ada.
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join(BIBLE_DB);
        let err = |e| db_error(&path, e);
        let mut conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(err)?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(err)?;
        conn.pragma_update(None, "synchronous", 2).map_err(err)?;
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(err)?;
        if version > SCHEMA_VERSION {
            return Err(format!(
                "{} dibuat oleh Lentera versi yang lebih baru. Perbarui Lentera.",
                path.display()
            ));
        }
        if version == 0 {
            let tx = conn.transaction().map_err(err)?;
            tx.execute_batch(SCHEMA).map_err(err)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(err)?;
            tx.commit().map_err(err)?;
        }
        Ok(Self { conn, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn err(&self) -> impl Fn(rusqlite::Error) -> String + '_ {
        |e| db_error(&self.path, e)
    }

    /// Semua terjemahan, urut menurut singkatan.
    pub fn translations(&self) -> Result<Vec<Translation>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, code, name, verses FROM translations ORDER BY code COLLATE NOCASE")
            .map_err(self.err())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Translation {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    verses: row.get::<_, i64>(3)?.max(0) as usize,
                })
            })
            .map_err(self.err())?;
        rows.collect::<Result<_, _>>().map_err(self.err())
    }

    /// Menyimpan terjemahan baru dalam satu transaksi. Singkatan wajib diisi,
    /// paling panjang 12 huruf, dan belum dipakai terjemahan lain.
    pub fn add(
        &mut self,
        code: &str,
        name: &str,
        source: &str,
        verses: &[Verse],
    ) -> Result<Translation, String> {
        let code = tidy(code);
        if code.is_empty() {
            return Err("Singkatan terjemahan belum diisi, misalnya TB.".into());
        }
        if code.chars().count() > CODE_MAX {
            return Err(format!("Singkatan paling panjang {CODE_MAX} huruf."));
        }
        let name = match tidy(name) {
            name if name.is_empty() => code.clone(),
            name => name.chars().take(NAME_MAX).collect(),
        };
        let taken: Option<String> = self
            .conn
            .query_row(
                "SELECT code FROM translations WHERE code = ?1",
                [&code],
                |row| row.get(0),
            )
            .optional()
            .map_err(self.err())?;
        if let Some(taken) = taken {
            return Err(format!(
                "Singkatan {taken} sudah dipakai. Pakai singkatan lain atau hapus terjemahan {taken} dulu."
            ));
        }
        if verses.is_empty() {
            return Err("Tidak ada ayat untuk disimpan.".into());
        }
        let path = self.path.clone();
        let err = |e| db_error(&path, e);
        let tx = self.conn.transaction().map_err(err)?;
        tx.execute(
            "INSERT INTO translations (code, name, source, verses, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![code, name, source, verses.len() as i64, now_ms()],
        )
        .map_err(err)?;
        let id = tx.last_insert_rowid();
        {
            let mut insert = tx
                .prepare(
                    "INSERT OR IGNORE INTO verses (translation, book, chapter, verse, last, text)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(err)?;
            for v in verses {
                insert
                    .execute(params![id, v.book, v.chapter, v.verse, v.last, v.text])
                    .map_err(err)?;
            }
        }
        tx.commit().map_err(err)?;
        Ok(Translation {
            id,
            code,
            name,
            verses: verses.len(),
        })
    }

    /// Menghapus terjemahan beserta ayatnya.
    pub fn remove(&mut self, id: i64) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM translations WHERE id = ?1", [id])
            .map_err(self.err())?;
        // Mengecilkan berkas lagi. Gagal di sini tidak membatalkan penghapusan.
        let _ = self.conn.execute_batch("VACUUM");
        Ok(())
    }

    /// Terjemahan yang terakhir dipakai.
    pub fn last_translation(&self) -> Option<i64> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'translation'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|value| value.parse().ok())
    }

    pub fn set_last_translation(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES ('translation', ?1)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [id.to_string()],
            )
            .map(|_| ())
            .map_err(self.err())
    }

    /// Jumlah pasal sebuah kitab di terjemahan (0 bila kitabnya tidak ada).
    pub fn chapter_count(&self, translation: i64, book: u8) -> Result<u16, String> {
        let max: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(chapter) FROM verses WHERE translation = ?1 AND book = ?2",
                params![translation, book],
                |row| row.get(0),
            )
            .map_err(self.err())?;
        Ok(max.unwrap_or(0).clamp(0, i64::from(u16::MAX)) as u16)
    }

    /// Nomor ayat terakhir sebuah pasal.
    pub fn last_verse(
        &self,
        translation: i64,
        book: u8,
        chapter: u16,
    ) -> Result<Option<u16>, String> {
        let max: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(last) FROM verses WHERE translation = ?1 AND book = ?2 AND chapter = ?3",
                params![translation, book, chapter],
                |row| row.get(0),
            )
            .map_err(self.err())?;
        Ok(max.map(|n| n.clamp(0, i64::from(u16::MAX)) as u16))
    }

    /// Semua ayat satu pasal.
    pub fn chapter(&self, translation: i64, book: u8, chapter: u16) -> Result<Vec<Verse>, String> {
        self.range(translation, book, (chapter, 1), (chapter, END))
    }

    /// Ayat dari `start` sampai `end` (pasal, ayat). Ayat gabungan ikut bila
    /// sebagian nomornya masuk rentang.
    fn range(
        &self,
        translation: i64,
        book: u8,
        start: (u16, u16),
        end: (u16, u16),
    ) -> Result<Vec<Verse>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT chapter, verse, last, text FROM verses
                 WHERE translation = ?1 AND book = ?2
                   AND (chapter > ?3 OR (chapter = ?3 AND last >= ?4))
                   AND (chapter < ?5 OR (chapter = ?5 AND verse <= ?6))
                 ORDER BY chapter, verse
                 LIMIT ?7",
            )
            .map_err(self.err())?;
        let rows = stmt
            .query_map(
                params![
                    translation,
                    book,
                    start.0,
                    start.1,
                    end.0,
                    end.1,
                    MAX_PASSAGE_VERSES as i64 + 1
                ],
                |row| {
                    Ok(Verse {
                        book,
                        chapter: row.get(0)?,
                        verse: row.get(1)?,
                        last: row.get(2)?,
                        text: row.get(3)?,
                    })
                },
            )
            .map_err(self.err())?;
        rows.collect::<Result<_, _>>().map_err(self.err())
    }

    /// Membaca referensi lalu mengambil ayatnya dari terjemahan.
    pub fn lookup(&self, translation: i64, input: &str) -> Result<Lookup, String> {
        let mut reference = parse(input)?;
        let book = reference.book;
        let chapters = self.chapter_count(translation, book.number)?;
        if chapters == 0 {
            return Err(format!("Kitab {} tidak ada di terjemahan ini.", book.name));
        }
        for span in &mut reference.spans {
            if span.start.0 > chapters {
                return Err(format!("{} hanya sampai pasal {chapters}.", book.name));
            }
            if span.end.0 > chapters {
                span.end = (chapters, END);
            }
        }
        reference.clamp(|chapter| {
            self.last_verse(translation, book.number, chapter)
                .ok()
                .flatten()
        });
        let mut verses = Vec::new();
        let mut seen = HashSet::new();
        for span in &reference.spans {
            for verse in self.range(translation, book.number, span.start, span.end)? {
                if seen.insert((verse.chapter, verse.verse)) {
                    verses.push(verse);
                }
            }
            if verses.len() > MAX_PASSAGE_VERSES {
                return Err(format!(
                    "Terlalu panjang. Paling banyak {MAX_PASSAGE_VERSES} ayat sekali tampil."
                ));
            }
        }
        if !reference.spans.is_empty() && verses.is_empty() {
            return Err(format!(
                "{} tidak ada di terjemahan ini.",
                reference.label()
            ));
        }
        let chapter = reference.first_chapter().min(chapters);
        let chapter_verses = self.chapter(translation, book.number, chapter)?;
        Ok(Lookup {
            label: reference.label(),
            reference,
            chapters,
            chapter,
            chapter_verses,
            verses,
        })
    }
}

/// Spasi dirapikan: tidak ada spasi di awal/akhir atau ganda.
fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn db_error(path: &Path, e: rusqlite::Error) -> String {
    format!("Basis data Alkitab {} bermasalah.\n{e}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bible::parse_bible;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "lentera-bible-{name}-{}-{}",
                std::process::id(),
                now_ms()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Terjemahan percobaan: Yohanes 3 (36 ayat, 5-6 digabung), Yohanes 4
    /// (54 ayat), dan Yudas 1 (25 ayat). Teksnya buatan, bukan kutipan.
    fn sample() -> Vec<Verse> {
        let mut tsv = String::new();
        for verse in 1..=36 {
            match verse {
                5 => tsv.push_str("43\t3\t5-6\tTeks Yohanes 3:5-6\n"),
                6 => {}
                _ => tsv.push_str(&format!("43\t3\t{verse}\tTeks Yohanes 3:{verse}\n")),
            }
        }
        for verse in 1..=54 {
            tsv.push_str(&format!("43\t4\t{verse}\tTeks Yohanes 4:{verse}\n"));
        }
        for verse in 1..=25 {
            tsv.push_str(&format!("65\t1\t{verse}\tTeks Yudas {verse}\n"));
        }
        parse_bible(&tsv).unwrap().verses
    }

    fn numbers(lookup: &Lookup) -> Vec<(u16, u16)> {
        lookup.verses.iter().map(|v| (v.chapter, v.verse)).collect()
    }

    #[test]
    fn add_list_and_reopen() {
        let dir = TempDir::new("add");
        let mut bible = Bible::open(&dir.0).unwrap();
        assert!(bible.translations().unwrap().is_empty());
        let added = bible
            .add(" TB ", "  Terjemahan   Contoh ", "contoh.tsv", &sample())
            .unwrap();
        assert_eq!(added.code, "TB");
        assert_eq!(added.name, "Terjemahan Contoh");
        assert_eq!(added.verses, 114);
        assert!(
            bible
                .add("tb", "", "x", &sample())
                .unwrap_err()
                .contains("sudah dipakai")
        );
        assert!(
            bible
                .add(" ", "", "x", &sample())
                .unwrap_err()
                .contains("belum diisi")
        );
        assert!(
            bible
                .add("SANGATPANJANGSEKALI", "", "x", &sample())
                .unwrap_err()
                .contains("12 huruf")
        );
        bible.set_last_translation(added.id).unwrap();
        drop(bible);

        let bible = Bible::open(&dir.0).unwrap();
        assert_eq!(bible.translations().unwrap(), std::slice::from_ref(&added));
        assert_eq!(bible.last_translation(), Some(added.id));
        assert_eq!(bible.chapter_count(added.id, 43).unwrap(), 4);
        assert_eq!(bible.chapter_count(added.id, 1).unwrap(), 0);
    }

    #[test]
    fn lookup_ranges_lists_and_merged_verses() {
        let dir = TempDir::new("lookup");
        let mut bible = Bible::open(&dir.0).unwrap();
        let id = bible.add("TB", "", "x", &sample()).unwrap().id;

        let found = bible.lookup(id, "yoh 3:16-18").unwrap();
        assert_eq!(found.label, "Yohanes 3:16-18");
        assert_eq!(numbers(&found), [(3, 16), (3, 17), (3, 18)]);
        assert_eq!(found.chapter, 3);
        assert_eq!(found.chapter_verses.len(), 35);
        assert_eq!(found.chapters, 4);

        // Ayat 6 ada di dalam ayat gabungan 5-6.
        let merged = bible.lookup(id, "Yoh 3:6").unwrap();
        assert_eq!(numbers(&merged), [(3, 5)]);
        assert_eq!(merged.verses[0].number(), "5-6");

        let across = bible.lookup(id, "Yoh 3:35-4:2").unwrap();
        assert_eq!(numbers(&across), [(3, 35), (3, 36), (4, 1), (4, 2)]);

        let list = bible.lookup(id, "Yoh 3:16, 18; 4:1").unwrap();
        assert_eq!(numbers(&list), [(3, 16), (3, 18), (4, 1)]);

        let clamped = bible.lookup(id, "Yoh 3:30-99").unwrap();
        assert_eq!(clamped.label, "Yohanes 3:30-36");
        assert_eq!(clamped.verses.len(), 7);

        let whole = bible.lookup(id, "Yoh 4").unwrap();
        assert_eq!(whole.label, "Yohanes 4");
        assert_eq!(whole.verses.len(), 54);

        let chapters = bible.lookup(id, "Yoh 3-9").unwrap();
        assert_eq!(chapters.label, "Yohanes 3-4");
        assert_eq!(chapters.verses.len(), 35 + 54);

        let jude = bible.lookup(id, "Yud 3-4").unwrap();
        assert_eq!(jude.label, "Yudas 1:3-4");
        assert_eq!(numbers(&jude), [(1, 3), (1, 4)]);

        let book_only = bible.lookup(id, "Yohanes").unwrap();
        assert!(book_only.verses.is_empty());
        assert_eq!(book_only.chapter, 1);
        assert!(book_only.chapter_verses.is_empty());
    }

    #[test]
    fn lookup_errors() {
        let dir = TempDir::new("errors");
        let mut bible = Bible::open(&dir.0).unwrap();
        let id = bible.add("TB", "", "x", &sample()).unwrap().id;
        assert!(
            bible
                .lookup(id, "Kej 1:1")
                .unwrap_err()
                .contains("Kejadian tidak ada")
        );
        assert!(
            bible
                .lookup(id, "Yoh 9:1")
                .unwrap_err()
                .contains("sampai pasal 4")
        );
        assert!(
            bible
                .lookup(id, "Yoh 3:40")
                .unwrap_err()
                .contains("tidak ada")
        );
        assert!(bible.lookup(id, "Yoh 1-4").is_ok());
        let mut many = String::from("Yoh 3:1");
        for verse in 2..=36 {
            many.push_str(&format!(",{verse}"));
        }
        assert!(bible.lookup(id, &many).is_ok());
    }

    #[test]
    fn remove_deletes_verses() {
        let dir = TempDir::new("remove");
        let mut bible = Bible::open(&dir.0).unwrap();
        let first = bible.add("TB", "", "x", &sample()).unwrap();
        let second = bible.add("TL", "", "y", &sample()[..10]).unwrap();
        bible.remove(first.id).unwrap();
        assert_eq!(bible.translations().unwrap(), std::slice::from_ref(&second));
        assert_eq!(bible.chapter_count(first.id, 43).unwrap(), 0);
        assert_eq!(bible.chapter_count(second.id, 43).unwrap(), 3);
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = TempDir::new("newer");
        drop(Bible::open(&dir.0).unwrap());
        let conn = Connection::open(dir.0.join(BIBLE_DB)).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        drop(conn);
        assert!(Bible::open(&dir.0).unwrap_err().contains("lebih baru"));
    }
}
