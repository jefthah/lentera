//! Membaca referensi seperti "Yoh 3:16-18", "Mzm 23", "Rm 8:28, 31-39",
//! atau "Yoh 3:16-4:2".

use super::books::{Book, BookMatch, find_book};

/// Nomor ayat penanda "sampai akhir pasal".
pub const END: u16 = u16::MAX;

/// Kitab yang hanya satu pasal. "Yud 3" berarti ayat 3, bukan pasal 3.
const SINGLE_CHAPTER: [u8; 5] = [31, 57, 63, 64, 65];

/// Batas jumlah bagian dalam satu referensi.
const MAX_SPANS: usize = 50;

/// Rentang ayat berurutan, dari `start` sampai `end` (keduanya ikut).
/// Masing-masing berupa (pasal, ayat); ayat [`END`] berarti sampai akhir pasal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: (u16, u16),
    pub end: (u16, u16),
}

impl Span {
    fn chapters(first: u16, last: u16) -> Self {
        Self {
            start: (first, 1),
            end: (last, END),
        }
    }

    /// Satu pasal utuh atau beberapa pasal utuh.
    fn whole_chapters(&self) -> bool {
        self.start.1 == 1 && self.end.1 == END
    }
}

/// Referensi yang sudah dibaca. `spans` kosong berarti hanya nama kitab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub book: &'static Book,
    pub spans: Vec<Span>,
}

impl Reference {
    /// Pasal pertama yang disebut (1 bila hanya nama kitab).
    pub fn first_chapter(&self) -> u16 {
        self.spans.first().map_or(1, |span| span.start.0)
    }

    /// Menyesuaikan akhir rentang dengan jumlah ayat sebenarnya, supaya
    /// "Yoh 3:16-99" tertulis "Yoh 3:16-36". `last_verse` memberi nomor ayat
    /// terakhir sebuah pasal.
    pub fn clamp(&mut self, last_verse: impl Fn(u16) -> Option<u16>) {
        for span in &mut self.spans {
            if span.end.1 != END
                && let Some(last) = last_verse(span.end.0)
                && span.end.1 > last
            {
                span.end.1 = last;
                if span.end < span.start {
                    span.end = span.start;
                }
            }
        }
    }

    /// Tulisan baku, misalnya "Yohanes 3:16-18" atau "Roma 8:28, 31-39".
    pub fn label(&self) -> String {
        let mut out = String::from(self.book.name);
        let mut chapter = None;
        for (index, span) in self.spans.iter().enumerate() {
            let (c1, v1) = span.start;
            let (c2, v2) = span.end;
            let same = chapter == Some(c1) && c1 == c2 && !span.whole_chapters();
            out.push_str(match (index, same) {
                (0, _) => " ",
                (_, true) => ", ",
                (_, false) => "; ",
            });
            if span.whole_chapters() {
                out.push_str(&c1.to_string());
                if c2 != c1 {
                    out.push_str(&format!("-{c2}"));
                }
                chapter = None;
                continue;
            }
            if !same {
                out.push_str(&format!("{c1}:"));
            }
            out.push_str(&v1.to_string());
            if c2 != c1 {
                out.push_str(&format!("-{c2}:{v2}"));
            } else if v2 != v1 {
                out.push_str(&format!("-{v2}"));
            }
            chapter = Some(c2);
        }
        out
    }
}

/// Membaca referensi. Pemisah pasal dan ayat boleh titik dua atau titik
/// ("Yoh 3.16"). Koma memisahkan ayat dalam pasal yang sama, titik koma
/// memisahkan pasal. Tanda pisah di akhir (sedang mengetik) diabaikan.
pub fn parse(input: &str) -> Result<Reference, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Ketik referensi, misalnya Yoh 3:16-18.".into());
    }
    let (name, numbers) = split_book(input);
    let book = match find_book(name) {
        BookMatch::Found(book) => book,
        BookMatch::NotFound => {
            return Err(format!(
                "Kitab \u{201c}{}\u{201d} tidak dikenal.",
                name.trim()
            ));
        }
        BookMatch::Ambiguous(books) => {
            let mut names: Vec<_> = books.iter().take(4).map(|b| b.name).collect();
            if books.len() > 4 {
                names.push("…");
            }
            return Err(format!(
                "\u{201c}{}\u{201d} bisa berarti {}. Ketik lebih lengkap.",
                name.trim(),
                names.join(", ")
            ));
        }
    };
    let spans = parse_spans(numbers, SINGLE_CHAPTER.contains(&book.number))?;
    Ok(Reference { book, spans })
}

/// Memisahkan nama kitab dari angka pasal/ayat. Angka di depan nama
/// ("1 Kor", "2Yoh") termasuk nama kitab.
fn split_book(input: &str) -> (&str, &str) {
    let lead = input.len() - input.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let after_lead = input[lead..].trim_start_matches([' ', '.']);
    let start = if lead > 0 && after_lead.starts_with(char::is_alphabetic) {
        input.len() - after_lead.len()
    } else {
        0
    };
    match input[start..].find(|c: char| c.is_ascii_digit()) {
        Some(at) if start + at > 0 => input.split_at(start + at),
        Some(_) => (input, ""),
        None => (input, ""),
    }
}

fn parse_spans(numbers: &str, single_chapter: bool) -> Result<Vec<Span>, String> {
    let text: String = numbers
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '.' => ':',
            '\u{2013}' | '\u{2014}' => '-',
            c => c,
        })
        .collect();
    let mut spans = Vec::new();
    // Pasal yang sedang disebut, bila bagian sebelumnya menyebut ayat.
    let mut verse_chapter: Option<u16> = None;
    let mut separator = ';';
    let mut rest = text.as_str();
    loop {
        let end = rest.find([',', ';']).unwrap_or(rest.len());
        let part = &rest[..end];
        if !part.is_empty() {
            let span = parse_part(part, separator, verse_chapter, single_chapter)?;
            verse_chapter = (!span.whole_chapters() || single_chapter).then_some(span.end.0);
            spans.push(span);
            if spans.len() > MAX_SPANS {
                return Err(format!(
                    "Paling banyak {MAX_SPANS} bagian dalam satu referensi."
                ));
            }
        }
        let Some(next) = rest[end..].chars().next() else {
            break;
        };
        separator = next;
        rest = &rest[end + 1..];
    }
    Ok(spans)
}

fn parse_part(
    part: &str,
    separator: char,
    verse_chapter: Option<u16>,
    single_chapter: bool,
) -> Result<Span, String> {
    let (from, to) = match part.split_once('-') {
        Some((from, to)) => (from, Some(to).filter(|to| !to.is_empty())),
        None => (part, None),
    };
    if to.is_some_and(|to| to.contains('-')) {
        return Err(format!("Rentang \u{201c}{part}\u{201d} tidak dikenali."));
    }
    let span = if let Some((chapter, verse)) = from.split_once(':') {
        let chapter = number(chapter, part)?;
        let Some(verse) = Some(verse).filter(|v| !v.is_empty()) else {
            // "Yoh 3:" saat masih mengetik: pasal 3.
            return Ok(Span::chapters(chapter, chapter));
        };
        let start = (chapter, number(verse, part)?);
        let end = match to {
            None => start,
            Some(to) => match to.split_once(':') {
                Some((c, v)) => (number(c, part)?, number(v, part)?),
                None => (chapter, number(to, part)?),
            },
        };
        Span { start, end }
    } else {
        let first = number(from, part)?;
        let last = to.map(|to| number(to, part)).transpose()?;
        let verse_chapter = if single_chapter {
            Some(1)
        } else {
            verse_chapter.filter(|_| separator == ',')
        };
        match verse_chapter {
            Some(chapter) => Span {
                start: (chapter, first),
                end: (chapter, last.unwrap_or(first)),
            },
            None => Span::chapters(first, last.unwrap_or(first)),
        }
    };
    if span.end < span.start {
        return Err(format!("Rentang \u{201c}{part}\u{201d} terbalik."));
    }
    Ok(span)
}

fn number(text: &str, part: &str) -> Result<u16, String> {
    if text.is_empty() || text.len() > 3 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("Referensi \u{201c}{part}\u{201d} tidak dikenali."));
    }
    match text.parse() {
        Ok(0) | Err(_) => Err("Nomor pasal dan ayat dimulai dari 1.".into()),
        Ok(n) => Ok(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(input: &str) -> Vec<((u16, u16), (u16, u16))> {
        parse(input)
            .unwrap()
            .spans
            .iter()
            .map(|s| (s.start, s.end))
            .collect()
    }

    fn label(input: &str) -> String {
        parse(input).unwrap().label()
    }

    #[test]
    fn common_forms() {
        assert_eq!(spans("Yoh 3:16"), [((3, 16), (3, 16))]);
        assert_eq!(spans("yoh 3:16-18"), [((3, 16), (3, 18))]);
        assert_eq!(spans("Yoh. 3.16 - 18"), [((3, 16), (3, 18))]);
        assert_eq!(spans("Yoh 3:16–4:2"), [((3, 16), (4, 2))]);
        assert_eq!(spans("Mzm 23"), [((23, 1), (23, END))]);
        assert_eq!(spans("Mzm 1-2"), [((1, 1), (2, END))]);
        assert_eq!(
            spans("Rm 8:28, 31-39"),
            [((8, 28), (8, 28)), ((8, 31), (8, 39))]
        );
        assert_eq!(
            spans("Yoh 3:16; 4:1-2"),
            [((3, 16), (3, 16)), ((4, 1), (4, 2))]
        );
        assert_eq!(
            spans("Mzm 23; 24"),
            [((23, 1), (23, END)), ((24, 1), (24, END))]
        );
        assert_eq!(spans("1Kor13:4-7"), [((13, 4), (13, 7))]);
        assert_eq!(spans("1 Kor 13:4"), [((13, 4), (13, 4))]);
        assert_eq!(parse("2 Yoh").unwrap().book.number, 63);
    }

    #[test]
    fn single_chapter_books_take_verses() {
        assert_eq!(spans("Yud 3"), [((1, 3), (1, 3))]);
        assert_eq!(spans("Yud 3-5"), [((1, 3), (1, 5))]);
        assert_eq!(spans("Flm 1:10"), [((1, 10), (1, 10))]);
        assert_eq!(spans("Yud 1:3, 5"), [((1, 3), (1, 3)), ((1, 5), (1, 5))]);
    }

    #[test]
    fn book_only_and_unfinished_typing() {
        assert!(parse("Yohanes").unwrap().spans.is_empty());
        assert_eq!(spans("Yoh 3:"), [((3, 1), (3, END))]);
        assert_eq!(spans("Yoh 3:16-"), [((3, 16), (3, 16))]);
        assert_eq!(spans("Yoh 3:16,"), [((3, 16), (3, 16))]);
    }

    #[test]
    fn errors_explain_the_problem() {
        assert!(parse("").unwrap_err().contains("Ketik referensi"));
        assert!(parse("Xyz 1:1").unwrap_err().contains("tidak dikenal"));
        assert!(
            parse("Ye 1")
                .unwrap_err()
                .contains("Yesaya, Yeremia, Yehezkiel")
        );
        assert!(parse("Yoh 3:18-16").unwrap_err().contains("terbalik"));
        assert!(parse("Yoh 0:1").unwrap_err().contains("dimulai dari 1"));
        assert!(parse("Yoh 3:1a").unwrap_err().contains("tidak dikenali"));
        assert!(parse("Yoh 1-2-3").unwrap_err().contains("tidak dikenali"));
    }

    #[test]
    fn labels_are_tidy() {
        assert_eq!(label("yoh 3:16"), "Yohanes 3:16");
        assert_eq!(label("yoh 3.16-18"), "Yohanes 3:16-18");
        assert_eq!(label("Mzm 23"), "Mazmur 23");
        assert_eq!(label("mzm 1-2"), "Mazmur 1-2");
        assert_eq!(label("Rm 8:28,31-39"), "Roma 8:28, 31-39");
        assert_eq!(label("Yoh 3:16;4:1"), "Yohanes 3:16; 4:1");
        assert_eq!(label("Yoh 3:16-4:2"), "Yohanes 3:16-4:2");
        assert_eq!(label("kpr 2"), "Kisah Para Rasul 2");
        assert_eq!(label("Yohanes"), "Yohanes");
    }

    #[test]
    fn clamp_shortens_ranges_past_the_last_verse() {
        let mut reference = parse("Yoh 3:16-99").unwrap();
        reference.clamp(|chapter| (chapter == 3).then_some(36));
        assert_eq!(reference.label(), "Yohanes 3:16-36");
        let mut reference = parse("Yoh 3:40-50").unwrap();
        reference.clamp(|_| Some(36));
        assert_eq!(reference.label(), "Yohanes 3:40");
    }
}
