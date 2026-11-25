//! Impor dari EasyWorship: lagu (EasyWorship 2009, 6, 7, dan 8) dan Alkitab
//! (`.ewb`).
//!
//! Berkas EasyWorship hanya dibaca, tidak pernah diubah. Basis data SQLite
//! dibuka dengan `immutable=1`: tanpa kunci dan tanpa berkas jurnal, jadi
//! aman walaupun EasyWorship sedang terbuka.
//!
//! Letak database lagu:
//!
//! - EasyWorship 6–8: `Songs.db` dan `SongWords.db` (SQLite) di
//!   `C:\Users\Public\Documents\Softouch\EasyWorship\Default\v6.1\Databases\Data`.
//!   Lirik berupa RTF.
//! - EasyWorship 2009: `Songs.DB` dan `Songs.MB` (Paradox) di
//!   `C:\Users\Public\Documents\Softouch\EasyWorship\Default\Databases\Data`.
//!
//! Alkitab `.ewb` ada di `C:\ProgramData\Softouch\EasyWorship\Plugins\Bibles`
//! (2009), `EasyWorship.v7\Plugins\Bibles`, atau `EasyWorship.v8\Plugins\Bibles`.

mod bible;
mod paradox;
mod rtf;

pub use bible::read_bible;

use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Versi EasyWorship asal database lagu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition {
    /// EasyWorship 2009 (Paradox).
    Ew2009,
    /// EasyWorship 6, 7, dan 8 (SQLite).
    Ew6,
}

impl Edition {
    pub fn name(self) -> &'static str {
        match self {
            Edition::Ew2009 => "EasyWorship 2009",
            Edition::Ew6 => "EasyWorship 6\u{2013}8",
        }
    }
}

/// Satu lagu: judul dan lirik dengan baris kosong di antara slide, seperti
/// lagu di pustaka Lentera.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    pub title: String,
    pub content: String,
}

/// Isi database lagu EasyWorship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongDatabase {
    pub edition: Edition,
    /// Lagu berlirik, urut judul.
    pub songs: Vec<Song>,
    /// Lagu tanpa lirik; tidak diikutkan di `songs`.
    pub empty: usize,
}

const NOT_SONGS: &str = "Berkas ini bukan database lagu EasyWorship. Pilih Songs.db di folder \
     Databases\\Data EasyWorship.";

/// Membaca semua lagu dari database EasyWorship. `path` boleh berkas
/// `Songs.db` (EasyWorship 6–8), `Songs.DB` (EasyWorship 2009),
/// `SongWords.db`, atau folder yang memuatnya.
pub fn read_songs(path: &Path) -> Result<SongDatabase, String> {
    let file = song_file(path)?;
    let mut head = [0u8; 16];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(&file).map_err(|e| e.to_string())?;
        let read = f.read(&mut head).map_err(|e| e.to_string())?;
        if read < head.len() {
            return Err(NOT_SONGS.into());
        }
    }
    let (edition, raw) = if head.starts_with(b"SQLite format 3") {
        (Edition::Ew6, read_sqlite_songs(&file)?)
    } else {
        (Edition::Ew2009, read_paradox_songs(&file)?)
    };
    let mut songs = Vec::new();
    let mut empty = 0;
    for (title, words) in raw {
        match song(&title, &words) {
            Some(song) => songs.push(song),
            None => empty += 1,
        }
    }
    songs.sort_by_cached_key(|s| s.title.to_lowercase());
    Ok(SongDatabase {
        edition,
        songs,
        empty,
    })
}

/// Database lagu EasyWorship yang ada di komputer ini, EasyWorship 6–8 dulu.
pub fn song_locations() -> Vec<PathBuf> {
    let Some(public) = std::env::var_os("PUBLIC") else {
        return Vec::new();
    };
    let root = Path::new(&public)
        .join("Documents")
        .join("Softouch")
        .join("EasyWorship");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut profiles: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Profil "Default" dulu, lalu urut nama.
    profiles.sort_by_key(|p| {
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        (name != "default", name)
    });
    let mut found = Vec::new();
    for data in [
        &["v6.1", "Databases", "Data"][..],
        &["Databases", "Data"][..],
    ] {
        for profile in &profiles {
            let dir = data.iter().fold(profile.clone(), |p, part| p.join(part));
            if let Some(file) = find_in_dir(&dir, "Songs.db") {
                found.push(file);
            }
        }
    }
    found
}

/// Folder Alkitab EasyWorship di komputer ini yang berisi berkas `.ewb`,
/// versi terbaru dulu: `C:\ProgramData\Softouch\EasyWorship.v8\Plugins\Bibles`,
/// lalu `.v7`, lalu EasyWorship 2009 (`EasyWorship\Plugins\Bibles`). Folder
/// ProgramData tersembunyi, jadi pemilih berkas dibuka langsung di sini.
pub fn bible_locations() -> Vec<PathBuf> {
    let Some(data) = std::env::var_os("ProgramData") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(Path::new(&data).join("Softouch")) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let dir = e.path().join("Plugins").join("Bibles");
            let has_bible = std::fs::read_dir(&dir)
                .ok()?
                .filter_map(Result::ok)
                .any(|f| {
                    Path::new(&f.file_name())
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("ewb"))
                });
            (name.starts_with("easyworship") && has_bible).then_some((name, dir))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, dir)| dir).collect()
}

/// Folder gambar EasyWorship 6–8 di komputer ini
/// (`%PUBLIC%\Documents\Softouch\EasyWorship\<profil>\Resources\Images`)
/// yang berisi gambar JPG atau PNG. Dipakai sebagai folder awal saat
/// menambahkan gambar latar.
pub fn image_locations() -> Vec<PathBuf> {
    resource_locations("Images", crate::background::is_supported)
}

/// Folder video EasyWorship 6–8 di komputer ini (`…\Resources\Videos`) yang
/// berisi video MP4, M4V, MOV, atau WMV. Dipakai sebagai folder awal saat
/// menambahkan latar bila tidak ada folder gambar.
pub fn video_locations() -> Vec<PathBuf> {
    resource_locations("Videos", crate::background::is_video_file)
}

/// Folder `Resources\<sub>` setiap profil EasyWorship yang berisi berkas
/// yang cocok dengan `wanted`.
fn resource_locations(sub: &str, wanted: fn(&Path) -> bool) -> Vec<PathBuf> {
    let Some(public) = std::env::var_os("PUBLIC") else {
        return Vec::new();
    };
    let root = Path::new(&public)
        .join("Documents")
        .join("Softouch")
        .join("EasyWorship");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path().join("Resources").join(sub))
        .filter(|dir| {
            std::fs::read_dir(dir)
                .is_ok_and(|files| files.filter_map(Result::ok).any(|f| wanted(&f.path())))
        })
        .collect();
    found.sort();
    found
}

/// Berkas `name` di `dir`, tanpa membedakan huruf besar/kecil.
fn find_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
        .filter(|p| p.is_file())
}

fn song_file(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() {
        return find_in_dir(path, "Songs.db").ok_or_else(|| {
            "Folder ini tidak berisi database lagu EasyWorship (Songs.db).".to_owned()
        });
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if name.eq_ignore_ascii_case("SongWords.db") {
        let dir = path.parent().unwrap_or(Path::new("."));
        return find_in_dir(dir, "Songs.db").ok_or_else(|| NOT_SONGS.to_owned());
    }
    if !path.is_file() {
        return Err("Berkas tidak ditemukan.".into());
    }
    Ok(path.to_owned())
}

fn read_sqlite_songs(file: &Path) -> Result<Vec<(String, String)>, String> {
    let dir = file.parent().unwrap_or(Path::new("."));
    let words_file = find_in_dir(dir, "SongWords.db").ok_or_else(|| {
        "SongWords.db (lirik) tidak ada di folder yang sama dengan Songs.db.".to_owned()
    })?;
    let text = |row: &rusqlite::Row, i: usize| -> rusqlite::Result<String> {
        Ok(match row.get_ref(i)? {
            ValueRef::Text(b) | ValueRef::Blob(b) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        })
    };

    let songs = open_sqlite(file)?;
    // Tanpa ORDER BY judul: kolomnya memakai collation khusus EasyWorship.
    let mut statement = songs
        .prepare("SELECT rowid, title FROM song ORDER BY rowid")
        .map_err(|_| NOT_SONGS.to_owned())?;
    let titles = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, text(row, 1)?)))
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        .map_err(|_| NOT_SONGS.to_owned())?;

    let words_db = open_sqlite(&words_file)?;
    let mut statement = words_db
        .prepare("SELECT song_id, words FROM word")
        .map_err(|_| "SongWords.db bukan berkas lirik EasyWorship.".to_owned())?;
    let mut words: HashMap<i64, String> = statement
        .query_map([], |row| Ok((row.get::<_, Option<i64>>(0)?, text(row, 1)?)))
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        .map_err(|_| "SongWords.db bukan berkas lirik EasyWorship.".to_owned())?
        .into_iter()
        .filter_map(|(id, words)| Some((id?, words)))
        .collect();

    Ok(titles
        .into_iter()
        .map(|(id, title)| (title, words.remove(&id).unwrap_or_default()))
        .collect())
}

fn read_paradox_songs(file: &Path) -> Result<Vec<(String, String)>, String> {
    let dir = file.parent().unwrap_or(Path::new("."));
    let db = std::fs::read(file).map_err(|e| e.to_string())?;
    let memo = match find_in_dir(dir, "Songs.MB") {
        Some(memo) => Some(std::fs::read(memo).map_err(|e| e.to_string())?),
        None => None,
    };
    let table = paradox::read(&db, memo.as_deref()).map_err(|e| {
        if e.starts_with("Berkas ini bukan") {
            NOT_SONGS.to_owned()
        } else {
            e
        }
    })?;
    if memo.is_none() {
        return Err("Songs.MB (lirik) tidak ada di folder yang sama dengan Songs.DB.".into());
    }
    let (Some(title), Some(words)) = (table.column("Title"), table.column("Words")) else {
        return Err(NOT_SONGS.into());
    };
    Ok(table
        .rows
        .iter()
        .map(|row| (row[title].text().to_owned(), row[words].text().to_owned()))
        .collect())
}

/// Lagu dari judul dan lirik RTF; `None` bila liriknya kosong. Karakter
/// kontrol liar di judul dibuang (yang di lirik dibuang pembaca RTF).
fn song(title: &str, words: &str) -> Option<Song> {
    let slides = rtf::slides(&rtf::paragraphs(words));
    let content = slides.join("\n\n");
    let first_line = content.lines().next()?.to_owned();
    let title = crate::clean_text(title);
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(Song {
        title: if title.is_empty() { first_line } else { title },
        content,
    })
}

/// Membuka basis data SQLite hanya untuk dibaca, tanpa kunci dan jurnal.
pub(crate) fn open_sqlite(path: &Path) -> Result<Connection, String> {
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from(if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for c in text.chars() {
        match c {
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3f"),
            '#' => uri.push_str("%23"),
            ' ' => uri.push_str("%20"),
            c => uri.push(c),
        }
    }
    uri.push_str("?immutable=1");
    Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("{} tidak bisa dibuka: {e}", path.display()))
}

/// Satu byte Windows-1252 sebagai karakter.
pub(crate) fn cp1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}',
        '\u{8f}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}',
        '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}',
        '\u{178}',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        _ => byte as char,
    }
}

/// Teks Windows-1252 sebagai `String`.
pub(crate) fn cp1252_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| cp1252(b)).collect()
}

#[cfg(test)]
mod tests {
    use super::paradox::tests::{alpha, long, memo_field, table};
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lentera-ew-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const EW7_WORDS: &str = concat!(
        r"{\rtf1\ansi\deff0\sdeasyworship2{\fonttbl{\f0 Tahoma;}}",
        r"{\pard\sdparawysiwghidden\qc Verse 1\par}",
        r"{\pard\qc Kasih-Mu Tuhan\par}",
        r"{\pard\sdslidemarker\qc\par}",
        r"{\pard\qc Sungguh ajaib\par}}"
    );

    #[test]
    fn easyworship_7_songs() {
        // Folder dengan spasi dan "#": letaknya harus ditulis benar di URI SQLite.
        let dir = temp_dir("ew7 #1");
        let songs = Connection::open(dir.join("Songs.db")).unwrap();
        songs
            .execute_batch(
                "CREATE TABLE song (rowid INTEGER PRIMARY KEY, title TEXT NOT NULL);
                 INSERT INTO song VALUES (1, 'Zaman  Baru'), (2, 'Kosong'), (3, '  '), (4, 'Amin');",
            )
            .unwrap();
        drop(songs);
        let words = Connection::open(dir.join("SongWords.db")).unwrap();
        words
            .execute_batch(
                "CREATE TABLE word (rowid INTEGER PRIMARY KEY, song_id INTEGER, words rtf);
                 INSERT INTO word VALUES (1, 2, ''), (2, 4, 'Amin amin'), (3, 99, 'yatim');",
            )
            .unwrap();
        words
            .execute("INSERT INTO word VALUES (4, 1, ?1)", [EW7_WORDS])
            .unwrap();
        words
            .execute("INSERT INTO word VALUES (5, 3, ?1)", [EW7_WORDS.as_bytes()])
            .unwrap();
        drop(words);

        for path in [dir.join("Songs.db"), dir.join("SongWords.db"), dir.clone()] {
            let db = read_songs(&path).unwrap();
            assert_eq!(db.edition, Edition::Ew6);
            assert_eq!(db.empty, 1);
            let songs: Vec<_> = db
                .songs
                .iter()
                .map(|s| (s.title.as_str(), s.content.as_str()))
                .collect();
            assert_eq!(
                songs,
                [
                    ("Amin", "Amin amin"),
                    // Judul kosong diganti baris pertama lirik.
                    ("Kasih-Mu Tuhan", "Kasih-Mu Tuhan\n\nSungguh ajaib"),
                    ("Zaman Baru", "Kasih-Mu Tuhan\n\nSungguh ajaib"),
                ]
            );
        }
        // Berkas EasyWorship tidak berubah dan tidak ada berkas jurnal baru.
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["SongWords.db", "Songs.db"]);

        std::fs::remove_file(dir.join("SongWords.db")).unwrap();
        assert!(read_songs(&dir).unwrap_err().contains("SongWords.db"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn easyworship_2009_songs() {
        let dir = temp_dir("ew2009");
        let words =
            b"{\\rtf1\\ansi\\deff0 Reff:\\par Caf\\'e9 kami\\par\\par Bait \\'93dua\\'94\\par}";
        let mut mb = vec![0u8; 0x100];
        mb.push(2);
        mb.extend([0u8; 8]);
        mb.extend(words);
        let fields = [("Title", 0x01, 20), ("RecID", 0x04, 4), ("Words", 0x0C, 20)];
        let records = vec![
            [
                alpha(b"Lagu \x93Satu\x94", 20),
                long(1),
                memo_field(20, 0x100, words.len()),
            ]
            .concat(),
            [alpha(b"Kosong", 20), long(2), vec![0u8; 20]].concat(),
        ];
        std::fs::write(dir.join("Songs.DB"), table(&fields, &records)).unwrap();
        assert!(
            read_songs(&dir.join("Songs.DB"))
                .unwrap_err()
                .contains("Songs.MB")
        );
        std::fs::write(dir.join("Songs.MB"), &mb).unwrap();

        let db = read_songs(&dir.join("Songs.DB")).unwrap();
        assert_eq!(db.edition, Edition::Ew2009);
        assert_eq!(db.empty, 1);
        assert_eq!(
            db.songs,
            [Song {
                title: "Lagu \u{201c}Satu\u{201d}".into(),
                content: "Caf\u{e9} kami\n\nBait \u{201c}dua\u{201d}".into(),
            }]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_other_files() {
        let dir = temp_dir("lain");
        std::fs::write(dir.join("Songs.db"), "bukan database lagu sama sekali").unwrap();
        assert_eq!(read_songs(&dir).unwrap_err(), NOT_SONGS);
        let other = dir.join("lain.db");
        Connection::open(&other)
            .unwrap()
            .execute_batch("CREATE TABLE x (a); INSERT INTO x VALUES (1);")
            .unwrap();
        std::fs::write(dir.join("SongWords.db"), "").unwrap();
        assert_eq!(read_songs(&other).unwrap_err(), NOT_SONGS);
        assert!(read_songs(&dir.join("tidak-ada.db")).is_err());
        let empty = temp_dir("kosong");
        assert!(read_songs(&empty).unwrap_err().contains("Songs.db"));
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_dir_all(empty).unwrap();
    }

    #[test]
    fn windows_1252() {
        assert_eq!(
            cp1252_string(b"A\x80\x93x\x94\xe9\xff"),
            "A\u{20ac}\u{201c}x\u{201d}\u{e9}\u{ff}"
        );
    }
}
