//! Alkitab EasyWorship (`.ewb`).
//!
//! - EasyWorship 6 sampai 8: basis data SQLite. Teks setiap kitab disimpan
//!   terkompresi zlib (UTF-8); kolom `verse_info` berisi letak setiap ayat.
//! - EasyWorship 2009: berkas biner berawalan "EasyWorship Bible Text" dengan
//!   66 catatan kitab; teks kitab terkompresi zlib (Windows-1252), satu ayat
//!   per baris `pasal:ayat teks`.

use super::{cp1252_string, open_sqlite};
use crate::bible::{ImportedBible, Verse, clean, finish};
use std::path::Path;

const NOT_EWB: &str = "Berkas ini bukan Alkitab EasyWorship (.ewb).";
const EW2009_MAGIC: &[u8] = b"EasyWorship Bible Text";
/// Batas ukuran teks satu kitab setelah dibuka.
const BOOK_LIMIT: usize = 16 * 1024 * 1024;

/// Membaca Alkitab EasyWorship 2009 sampai 8.
pub fn read_bible(path: &Path) -> Result<ImportedBible, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > 64 * 1024 * 1024 {
        return Err("Berkas terlalu besar untuk Alkitab EasyWorship.".into());
    }
    let mut head = [0u8; 22];
    {
        use std::io::Read;
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let read = file.read(&mut head).map_err(|e| e.to_string())?;
        if read < head.len() {
            return Err(NOT_EWB.into());
        }
    }
    if head.starts_with(b"SQLite format 3") {
        finish(read_sqlite(path)?)
    } else if head.starts_with(EW2009_MAGIC) {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        finish(read_2009(&data)?)
    } else {
        Err(NOT_EWB.into())
    }
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, BOOK_LIMIT)
        .map_err(|_| "Teks Alkitab di berkas ini rusak.".to_owned())
}

fn read_sqlite(path: &Path) -> Result<ImportedBible, String> {
    let conn = open_sqlite(path)?;
    let (name, code): (String, String) = conn
        .query_row(
            "SELECT name, abbrev_name FROM header ORDER BY rowid LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| NOT_EWB.to_owned())?;
    let mut statement = conn
        .prepare(
            "SELECT b.verse_info, s.stream FROM books b \
             JOIN streams s ON s.rowid = b.rowid ORDER BY b.rowid",
        )
        .map_err(|_| NOT_EWB.to_owned())?;
    let books = statement
        .query_map([], |row| {
            Ok((row.get::<_, Option<Vec<u8>>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        .map_err(|_| NOT_EWB.to_owned())?;

    let mut verses = Vec::new();
    let mut skipped = 0;
    for (info, stream) in books {
        let Some(info) = info else { continue };
        let text = inflate(&stream)?;
        let units: Vec<u16> = String::from_utf8_lossy(&text).encode_utf16().collect();
        let (entries, _) = info.as_chunks::<8>();
        for entry in entries {
            let place = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]);
            let reference = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]);
            // Letak teks dalam satuan UTF-16, setelah dua karakter awalan.
            let start = 2 + (place >> 10) as usize;
            let len = (place & 0x3FF) as usize;
            let book = ((reference >> 18) & 0x7F) as u8;
            let chapter = ((reference >> 10) & 0xFF) as u16;
            let verse = ((reference >> 2) & 0xFF) as u16;
            let Some(slice) = units.get(start..start + len) else {
                skipped += 1;
                continue;
            };
            if !(1..=66).contains(&book) || chapter == 0 || verse == 0 {
                skipped += 1;
                continue;
            }
            verses.push(Verse {
                book,
                chapter,
                verse,
                last: verse,
                text: clean(&String::from_utf16_lossy(slice)),
            });
        }
    }
    Ok(ImportedBible {
        format: "EasyWorship 6\u{2013}8",
        name: Some(name.trim().to_owned()).filter(|n| !n.is_empty()),
        code: Some(code.trim().to_owned()).filter(|c| !c.is_empty()),
        verses,
        skipped,
    })
}

fn read_2009(data: &[u8]) -> Result<ImportedBible, String> {
    const FIRST_BOOK: usize = 0x58;
    const BOOK_RECORD: usize = 224;
    let mut verses: Vec<Verse> = Vec::new();
    let mut skipped = 0;
    for index in 0..66 {
        let record = FIRST_BOOK + index * BOOK_RECORD;
        let Some(entry) = data.get(record..record + BOOK_RECORD) else {
            break;
        };
        let offset = i64::from_le_bytes(entry[0xD0..0xD8].try_into().unwrap_or_default());
        let length = i64::from_le_bytes(entry[0xD8..0xE0].try_into().unwrap_or_default());
        if entry[51] == 0 || offset <= 0 || length <= 0 {
            continue;
        }
        let (offset, length) = (offset as usize, length as usize);
        let Some(stream) = data.get(offset..offset.saturating_add(length)) else {
            continue;
        };
        let text = cp1252_string(&inflate(stream)?);
        let book = index as u8 + 1;
        let mut current: Option<usize> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match verse_line(line) {
                Some((chapter, verse, rest)) => {
                    verses.push(Verse {
                        book,
                        chapter,
                        verse,
                        last: verse,
                        text: clean(rest),
                    });
                    current = Some(verses.len() - 1);
                }
                // Lanjutan ayat sebelumnya.
                None => match current {
                    Some(i) => verses[i].text = clean(&format!("{} {line}", verses[i].text)),
                    None => skipped += 1,
                },
            }
        }
    }
    Ok(ImportedBible {
        format: "EasyWorship 2009",
        name: None,
        code: None,
        verses,
        skipped,
    })
}

/// `"3:16 Karena begitu besar"` menjadi `(3, 16, "Karena begitu besar")`.
fn verse_line(line: &str) -> Option<(u16, u16, &str)> {
    let (chapter, rest) = line.split_once(':')?;
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let chapter: u16 = chapter.parse().ok().filter(|n| *n > 0)?;
    let verse: u16 = rest[..digits].parse().ok().filter(|n| *n > 0)?;
    Some((chapter, verse, rest[digits..].trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use miniz_oxide::deflate::compress_to_vec_zlib;
    use rusqlite::Connection;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lentera-ewb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn reference(book: u32, chapter: u32, verse: u32) -> u32 {
        (book << 18) | (chapter << 10) | (verse << 2)
    }

    #[test]
    fn easyworship_7_sqlite_bible() {
        let dir = temp_dir("sqlite");
        let path = dir.join("TB.ewb");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE header (rowid INTEGER PRIMARY KEY, name TEXT, abbrev_name TEXT);
             CREATE TABLE books (rowid INTEGER PRIMARY KEY, name TEXT, verse_info BLOB);
             CREATE TABLE streams (rowid INTEGER PRIMARY KEY, stream BLOB);
             INSERT INTO header VALUES (1, ' Alkitab Contoh ', 'AC');",
        )
        .unwrap();
        // Dua karakter awalan, lalu ayat-ayat; "”" dihitung satu satuan UTF-16.
        let text = "%KPada mulanya. \u{201d}Jadilah\u{201d} terang. Ayat 😀 tiga.";
        let verses = [
            (0u32, 14u32, reference(1, 1, 1)),
            (14, 18, reference(1, 1, 2)),
            (32, 13, reference(1, 1, 3)),
            (0, 4, reference(70, 1, 1)),
            (900, 4, reference(1, 1, 4)),
        ];
        let mut info = Vec::new();
        for (offset, len, r) in verses {
            info.extend(((offset << 10) | len).to_le_bytes());
            info.extend(r.to_le_bytes());
        }
        conn.execute("INSERT INTO books VALUES (1, 'Kejadian', ?1)", [&info])
            .unwrap();
        conn.execute(
            "INSERT INTO streams VALUES (1, ?1)",
            [compress_to_vec_zlib(text.as_bytes(), 6)],
        )
        .unwrap();
        drop(conn);

        let bible = read_bible(&path).unwrap();
        assert_eq!(bible.format, "EasyWorship 6\u{2013}8");
        assert_eq!(bible.name.as_deref(), Some("Alkitab Contoh"));
        assert_eq!(bible.code.as_deref(), Some("AC"));
        let texts: Vec<_> = bible.verses.iter().map(|v| v.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Pada mulanya.",
                "\u{201d}Jadilah\u{201d} terang.",
                "Ayat 😀 tiga."
            ]
        );
        assert_eq!(bible.skipped, 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn easyworship_2009_binary_bible() {
        let mut data = EW2009_MAGIC.to_vec();
        data.resize(0x58 + 66 * 224, 0);
        let books: [(usize, &[u8]); 2] = [
            (
                0,
                b"1:1 Pada mulanya\r\n\r\n1:2 Bumi belum\r\nberbentuk\r\n2:1 Demikianlah \x93diselesaikan\x94",
            ),
            (42, b"1:1 Kitab Yunus"),
        ];
        for (index, text) in books {
            let stream = compress_to_vec_zlib(text, 6);
            let record = 0x58 + index * 224;
            let offset = data.len() as i64;
            data[record] = b'K';
            data[record + 51] = 1;
            data[record + 0xD0..record + 0xD8].copy_from_slice(&offset.to_le_bytes());
            data[record + 0xD8..record + 0xE0]
                .copy_from_slice(&(stream.len() as i64).to_le_bytes());
            data.extend(stream);
        }
        let dir = temp_dir("2009");
        let path = dir.join("tb.ewb");
        std::fs::write(&path, &data).unwrap();

        let bible = read_bible(&path).unwrap();
        assert_eq!(bible.format, "EasyWorship 2009");
        assert_eq!(bible.name, None);
        let verses: Vec<_> = bible
            .verses
            .iter()
            .map(|v| (v.book, v.chapter, v.verse, v.text.as_str()))
            .collect();
        assert_eq!(
            verses,
            [
                (1, 1, 1, "Pada mulanya"),
                (1, 1, 2, "Bumi belum berbentuk"),
                (1, 2, 1, "Demikianlah \u{201c}diselesaikan\u{201d}"),
                (43, 1, 1, "Kitab Yunus"),
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_other_files() {
        let dir = temp_dir("lain");
        let path = dir.join("lain.ewb");
        std::fs::write(&path, "bukan alkitab sama sekali, hanya teks").unwrap();
        assert_eq!(read_bible(&path).unwrap_err(), NOT_EWB);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
