//! Membaca berkas Alkitab: OpenSong XML, Zefania XML, TSV, dan EasyWorship
//! (lihat [`crate::easyworship`]).

use super::Verse;
use super::books::{BOOKS, BookMatch, find_book};
use super::reference;
use crate::read_text_file;
use roxmltree::{Document, Node, ParsingOptions};
use std::path::Path;

/// Isi berkas Alkitab yang sudah dibaca, sebelum disimpan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedBible {
    /// Format berkas, untuk ditampilkan: "OpenSong XML", "Zefania XML", "TSV",
    /// "EasyWorship 2009", "EasyWorship 6–8".
    pub format: &'static str,
    /// Nama terjemahan dari isi berkas, bila ada.
    pub name: Option<String>,
    /// Singkatan dari isi berkas, bila ada.
    pub code: Option<String>,
    /// Urut menurut kitab, pasal, dan ayat; tanpa nomor ganda.
    pub verses: Vec<Verse>,
    /// Ayat yang dilewati: kitabnya di luar 66 kitab, nomornya ganda, atau
    /// teksnya kosong.
    pub skipped: usize,
}

impl ImportedBible {
    /// Jumlah kitab yang punya ayat.
    pub fn books(&self) -> usize {
        let mut books: Vec<u8> = self.verses.iter().map(|v| v.book).collect();
        books.dedup();
        books.len()
    }
}

/// Membaca isi berkas Alkitab. Formatnya dikenali dari isinya: XML dengan
/// elemen akar `bible` (OpenSong) atau `XMLBIBLE` (Zefania), selain itu TSV.
/// Seluruh berkas diperiksa dulu; tidak ada yang disimpan bila ada galat.
pub fn parse_bible(input: &str) -> Result<ImportedBible, String> {
    let input = input.trim_start_matches('\u{feff}');
    finish(if input.trim_start().starts_with('<') {
        parse_xml(input)?
    } else {
        parse_tsv(input)?
    })
}

/// Membaca berkas Alkitab. Berkas `.ewb` dibaca sebagai Alkitab EasyWorship,
/// berkas lain sebagai teks (lihat [`parse_bible`]).
pub fn read_bible_file(path: &Path) -> Result<ImportedBible, String> {
    let ewb = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ewb"));
    if ewb {
        crate::easyworship::read_bible(path)
    } else {
        parse_bible(&read_text_file(path)?)
    }
}

/// Mengurutkan ayat, membuang ayat kosong dan nomor ganda, lalu memastikan
/// masih ada ayat.
pub(crate) fn finish(mut bible: ImportedBible) -> Result<ImportedBible, String> {
    let before = bible.verses.len();
    bible.verses.retain(|v| !v.text.is_empty());
    bible.verses.sort_by_key(|v| (v.book, v.chapter, v.verse));
    bible.verses.dedup_by_key(|v| (v.book, v.chapter, v.verse));
    bible.skipped += before - bible.verses.len();
    if bible.verses.is_empty() {
        return Err("Tidak ada ayat yang dikenali di berkas ini.".into());
    }
    Ok(bible)
}

/// Merapikan spasi: baris baru dan spasi ganda menjadi satu spasi.
pub(crate) fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Nomor ayat "5" atau ayat gabungan "5-6".
fn verse_numbers(text: &str) -> Option<(u16, u16)> {
    let text = text.trim();
    let (first, last) = match text.split_once(['-', '\u{2013}']) {
        Some((first, last)) => (first.trim(), last.trim()),
        None => (text, text),
    };
    let first: u16 = first.parse().ok().filter(|n| *n > 0)?;
    let last: u16 = last.parse().ok().filter(|n| *n >= first).unwrap_or(first);
    Some((first, last))
}

fn book_number(name: &str) -> Option<u8> {
    if let Ok(number) = name.trim().parse::<u8>() {
        return (1..=66).contains(&number).then_some(number);
    }
    match find_book(name) {
        BookMatch::Found(book) => Some(book.number),
        _ => None,
    }
}

// ----- XML -----

fn parse_xml(input: &str) -> Result<ImportedBible, String> {
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let doc = Document::parse_with_options(input, options)
        .map_err(|e| format!("Berkas XML tidak valid.\n{e}"))?;
    let root = doc.root_element();
    if is(root, "XMLBIBLE") {
        Ok(zefania(root))
    } else if is(root, "bible") {
        Ok(opensong(root))
    } else {
        Err(format!(
            "Format XML <{}> belum didukung. Gunakan OpenSong XML atau Zefania XML.",
            root.tag_name().name()
        ))
    }
}

fn is(node: Node, tag: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(tag)
}

fn elements<'a, 'input>(
    node: Node<'a, 'input>,
    tags: &'static [&'static str],
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children()
        .filter(move |child| tags.iter().any(|tag| is(*child, tag)))
}

fn attr<'a>(node: Node<'a, '_>, names: &[&str]) -> Option<&'a str> {
    node.attributes()
        .find(|a| names.iter().any(|n| a.name().eq_ignore_ascii_case(n)))
        .map(|a| a.value())
}

/// Teks di dalam elemen, tanpa catatan kaki dan rujukan silang.
fn text_of(node: Node) -> String {
    fn collect(node: Node, out: &mut String) {
        for child in node.children() {
            if child.is_text() {
                out.push_str(child.text().unwrap_or_default());
            } else if child.is_element() {
                let tag = child.tag_name().name();
                if ["NOTE", "XREF", "MEDIA"]
                    .iter()
                    .any(|t| tag.eq_ignore_ascii_case(t))
                {
                    continue;
                }
                if tag.eq_ignore_ascii_case("BR") {
                    out.push(' ');
                }
                collect(child, out);
            }
        }
    }
    let mut out = String::new();
    collect(node, &mut out);
    clean(&out)
}

/// Pasal dan ayat di dalam satu kitab XML.
fn chapters(
    book: Node,
    number: u8,
    chapter_tags: &'static [&'static str],
    chapter_attr: &[&str],
    verse_tags: &'static [&'static str],
    verse_attr: &[&str],
    out: &mut Vec<Verse>,
) {
    for (index, chapter) in elements(book, chapter_tags).enumerate() {
        let chapter_number = attr(chapter, chapter_attr)
            .and_then(|n| n.trim().parse().ok())
            .filter(|n| *n > 0)
            .unwrap_or(index as u16 + 1);
        let mut previous = 0;
        for verse in elements(chapter, verse_tags) {
            let (first, last) = attr(verse, verse_attr)
                .and_then(verse_numbers)
                .unwrap_or((previous + 1, previous + 1));
            previous = last;
            out.push(Verse {
                book: number,
                chapter: chapter_number,
                verse: first,
                last,
                text: text_of(verse),
            });
        }
    }
}

/// OpenSong: `<bible><b n="Kejadian"><c n="1"><v n="1">…</v>`. Kitab
/// dikenali dari namanya. Bila ada nama yang tidak dikenal (misalnya bahasa
/// daerah) tetapi berkas berisi tepat 66 kitab, urutannya yang dipakai.
fn opensong(root: Node) -> ImportedBible {
    let books: Vec<_> = elements(root, &["b", "book"]).collect();
    let by_order = books.len() == BOOKS.len();
    let mut verses = Vec::new();
    let mut skipped = 0;
    for (index, book) in books.iter().enumerate() {
        let number = attr(*book, &["n", "name"])
            .and_then(book_number)
            .or_else(|| by_order.then_some(index as u8 + 1));
        let mut found = Vec::new();
        chapters(
            *book,
            number.unwrap_or(0),
            &["c", "chapter"],
            &["n", "number"],
            &["v", "verse"],
            &["n", "number"],
            &mut found,
        );
        if number.is_some() {
            verses.extend(found);
        } else {
            skipped += found.len();
        }
    }
    ImportedBible {
        format: "OpenSong XML",
        name: attr(root, &["n", "name", "title"]).map(clean),
        code: None,
        verses,
        skipped,
    }
}

/// Zefania: `<XMLBIBLE><BIBLEBOOK bnumber="1"><CHAPTER cnumber="1">
/// <VERS vnumber="1">…</VERS>`. Kitab bernomor di atas 66 (deuterokanonika)
/// dilewati.
fn zefania(root: Node) -> ImportedBible {
    let mut verses = Vec::new();
    let mut skipped = 0;
    for book in elements(root, &["BIBLEBOOK"]) {
        let number = match attr(book, &["bnumber"]).map(|n| n.trim().parse::<u32>()) {
            Some(Ok(n)) => u8::try_from(n).ok().filter(|n| (1..=66).contains(n)),
            _ => attr(book, &["bname", "bsname"]).and_then(book_number),
        };
        let mut found = Vec::new();
        chapters(
            book,
            number.unwrap_or(0),
            &["CHAPTER"],
            &["cnumber"],
            &["VERS"],
            &["vnumber"],
            &mut found,
        );
        if number.is_some() {
            verses.extend(found);
        } else {
            skipped += found.len();
        }
    }
    let info = elements(root, &["INFORMATION"]).next();
    let info_text = |tag: &'static [&'static str]| {
        info.and_then(|info| elements(info, tag).next())
            .map(text_of)
            .filter(|t| !t.is_empty())
    };
    ImportedBible {
        format: "Zefania XML",
        name: info_text(&["title"]).or_else(|| attr(root, &["biblename"]).map(clean)),
        code: info_text(&["identifier"]),
        verses,
        skipped,
    }
}

// ----- TSV -----

/// TSV UTF-8, satu ayat per baris, dalam salah satu bentuk:
///
/// - `kitab [TAB] pasal [TAB] ayat [TAB] teks`. Kitab berupa nomor 1–66 atau
///   nama/singkatan. Baris judul kolom di awal dilewati.
/// - `referensi [TAB] teks`, misalnya `Yoh 3:16 [TAB] …` (format impor ayat
///   Lentera).
///
/// Baris kosong dan baris yang diawali `#` diabaikan.
fn parse_tsv(input: &str) -> Result<ImportedBible, String> {
    let mut verses = Vec::new();
    let mut first = true;
    for (index, line) in input.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let line_no = index + 1;
        let columns: Vec<&str> = line.split('\t').collect();
        let header = std::mem::take(&mut first)
            && columns.len() >= 4
            && columns[1].trim().parse::<u16>().is_err();
        if header {
            continue;
        }
        let verse = if columns.len() >= 4 {
            let book = book_number(columns[0]).ok_or_else(|| {
                format!(
                    "Baris {line_no}: kitab \u{201c}{}\u{201d} tidak dikenal.",
                    columns[0].trim()
                )
            })?;
            let chapter = columns[1]
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("Baris {line_no}: nomor pasal tidak valid."))?;
            let (verse, last) = verse_numbers(columns[2])
                .ok_or_else(|| format!("Baris {line_no}: nomor ayat tidak valid."))?;
            Verse {
                book,
                chapter,
                verse,
                last,
                text: clean(&columns[3..].join(" ")),
            }
        } else if columns.len() >= 2 {
            let reference =
                reference::parse(columns[0]).map_err(|e| format!("Baris {line_no}: {e}"))?;
            let [span] = reference.spans.as_slice() else {
                return Err(format!(
                    "Baris {line_no}: referensi harus menunjuk satu ayat, misalnya Yoh 3:16."
                ));
            };
            if span.start.0 != span.end.0 || span.end.1 == reference::END {
                return Err(format!(
                    "Baris {line_no}: referensi harus menunjuk satu ayat, misalnya Yoh 3:16."
                ));
            }
            Verse {
                book: reference.book.number,
                chapter: span.start.0,
                verse: span.start.1,
                last: span.end.1,
                text: clean(&columns[1..].join(" ").replace("\\n", " ")),
            }
        } else {
            return Err(format!(
                "Baris {line_no} harus berisi kitab, pasal, ayat, dan teks yang dipisah TAB."
            ));
        };
        verses.push(verse);
    }
    Ok(ImportedBible {
        format: "TSV",
        name: None,
        code: None,
        verses,
        skipped: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(bible: &ImportedBible) -> Vec<(u8, u16, u16, u16, &str)> {
        bible
            .verses
            .iter()
            .map(|v| (v.book, v.chapter, v.verse, v.last, v.text.as_str()))
            .collect()
    }

    #[test]
    fn opensong_by_name_with_merged_verses() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <bible>
              <b n="Kejadian"><c n="1">
                <v n="1">Teks  satu
                  baris dua</v>
                <v n="2-3">Teks gabungan</v>
                <v n="4"></v>
              </c></b>
              <b n="Tobit"><c n="1"><v n="1">Di luar 66 kitab</v></c></b>
              <b n="John"><c n="3"><v n="16">Teks Yohanes</v></c></b>
            </bible>"#;
        let bible = parse_bible(xml).unwrap();
        assert_eq!(bible.format, "OpenSong XML");
        assert_eq!(
            refs(&bible),
            [
                (1, 1, 1, 1, "Teks satu baris dua"),
                (1, 1, 2, 3, "Teks gabungan"),
                (43, 3, 16, 16, "Teks Yohanes"),
            ]
        );
        assert_eq!(bible.skipped, 2);
        assert_eq!(bible.books(), 2);
    }

    #[test]
    fn opensong_with_66_unknown_names_uses_order() {
        let mut xml = String::from("<bible>");
        for n in 1..=66 {
            xml.push_str(&format!(
                r#"<b n="Kitab Daerah {n}"><c n="1"><v n="1">Teks {n}</v></c></b>"#
            ));
        }
        xml.push_str("</bible>");
        let bible = parse_bible(&xml).unwrap();
        assert_eq!(bible.verses.len(), 66);
        assert_eq!(bible.verses[42].book, 43);
        assert_eq!(bible.verses[42].text, "Teks 43");
    }

    #[test]
    fn zefania_numbers_info_and_notes() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
            <XMLBIBLE biblename="Contoh">
              <INFORMATION><title>Terjemahan Contoh</title><identifier>TC</identifier></INFORMATION>
              <BIBLEBOOK bnumber="43" bname="Yohanes"><CHAPTER cnumber="3">
                <VERS vnumber="16">Teks <STYLE css="x">tebal</STYLE><NOTE>catatan</NOTE> akhir</VERS>
                <VERS vnumber="16">Ganda</VERS>
              </CHAPTER></BIBLEBOOK>
              <BIBLEBOOK bnumber="70" bname="Sirakh"><CHAPTER cnumber="1"><VERS vnumber="1">x</VERS></CHAPTER></BIBLEBOOK>
            </XMLBIBLE>"#;
        let bible = parse_bible(xml).unwrap();
        assert_eq!(bible.format, "Zefania XML");
        assert_eq!(bible.name.as_deref(), Some("Terjemahan Contoh"));
        assert_eq!(bible.code.as_deref(), Some("TC"));
        assert_eq!(refs(&bible), [(43, 3, 16, 16, "Teks tebal akhir")]);
        assert_eq!(bible.skipped, 2);
    }

    #[test]
    fn tsv_four_columns_with_header_and_references() {
        let tsv = "\u{feff}kitab\tpasal\tayat\tteks\n\
                   # komentar\n\
                   43\t3\t16\tTeks satu\n\
                   Yoh\t3\t17-18\tTeks dua\n\
                   \n\
                   Rm 8:28\tTeks tiga\\nlanjut\n";
        let bible = parse_bible(tsv).unwrap();
        assert_eq!(bible.format, "TSV");
        assert_eq!(
            refs(&bible),
            [
                (43, 3, 16, 16, "Teks satu"),
                (43, 3, 17, 18, "Teks dua"),
                (45, 8, 28, 28, "Teks tiga lanjut"),
            ]
        );
    }

    #[test]
    fn errors_name_the_line_and_change_nothing() {
        assert!(
            parse_bible("43\t3\t16\tA\nXyz\t1\t1\tB")
                .unwrap_err()
                .starts_with("Baris 2: kitab")
        );
        assert!(
            parse_bible("Yoh 3\tSatu pasal")
                .unwrap_err()
                .contains("satu ayat")
        );
        assert!(
            parse_bible("hanya satu kolom")
                .unwrap_err()
                .contains("Baris 1")
        );
        assert!(
            parse_bible("<bible>")
                .unwrap_err()
                .contains("XML tidak valid")
        );
        assert!(
            parse_bible("<html></html>")
                .unwrap_err()
                .contains("belum didukung")
        );
        assert!(
            parse_bible("<bible></bible>")
                .unwrap_err()
                .contains("Tidak ada ayat")
        );
        assert!(
            parse_bible("# kosong\n")
                .unwrap_err()
                .contains("Tidak ada ayat")
        );
    }
}
