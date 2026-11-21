//! Alkitab: daftar kitab, referensi, impor, penyimpanan, dan slide ayat.
//!
//! Lentera tidak membawa teks Alkitab. Terjemahan diimpor sekali dari berkas
//! yang berhak dipakai pengguna (OpenSong XML, Zefania XML, atau TSV), lalu
//! disimpan di `alkitab.db` di folder data. Alkitab berlaku untuk semua profil
//! di komputer ini dan tidak ikut disinkronkan.

mod books;
mod import;
mod reference;
mod store;

pub use books::{BOOKS, Book, BookMatch, book, find_book};
pub use import::{ImportedBible, parse_bible};
pub use reference::{END, Reference, Span, parse as parse_reference};
pub use store::{BIBLE_DB, Bible, Lookup, MAX_PASSAGE_VERSES, Translation};

/// Satu ayat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verse {
    /// 1–66, lihat [`BOOKS`].
    pub book: u8,
    pub chapter: u16,
    /// Nomor ayat. Untuk ayat gabungan ("5-6"), nomor pertamanya.
    pub verse: u16,
    /// Nomor ayat terakhir; sama dengan `verse` kecuali ayat gabungan.
    pub last: u16,
    pub text: String,
}

impl Verse {
    /// Nomor ayat untuk ditampilkan: "16" atau "5-6".
    pub fn number(&self) -> String {
        if self.last > self.verse {
            format!("{}-{}", self.verse, self.last)
        } else {
            self.verse.to_string()
        }
    }
}

/// Cara membagi ayat ke slide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Packing {
    /// 1–3 ayat per slide menurut panjangnya.
    #[default]
    Auto,
    /// Satu ayat per slide.
    One,
}

/// Batas huruf satu slide pada [`Packing::Auto`]. Ayat yang lebih panjang
/// tetap satu slide.
const SLIDE_CHARS: usize = 220;
/// Batas ayat satu slide pada [`Packing::Auto`].
const SLIDE_VERSES: usize = 3;

/// Isi materi untuk ayat-ayat ini: nomor ayat kecil di depan setiap ayat,
/// baris kosong di antara slide. Baris dipotong supaya teks di layar 16:9
/// sebesar mungkin (ukuran teks slide mengikuti baris terpanjang).
pub fn passage_text(verses: &[Verse], packing: Packing) -> String {
    let mut slides: Vec<Vec<String>> = Vec::new();
    let mut chars = 0;
    let mut chapter = None;
    for verse in verses {
        let paragraph = format!("{}{}", superscript(&verse.number()), verse.text);
        let length = paragraph.chars().count();
        let fits = match slides.last() {
            Some(slide) => {
                packing == Packing::Auto
                    && chapter == Some(verse.chapter)
                    && slide.len() < SLIDE_VERSES
                    && chars + length <= SLIDE_CHARS
            }
            None => false,
        };
        match slides.last_mut() {
            Some(slide) if fits => {
                chars += length;
                slide.push(paragraph);
            }
            _ => {
                chars = length;
                slides.push(vec![paragraph]);
            }
        }
        chapter = Some(verse.chapter);
    }
    slides
        .iter()
        .map(|paragraphs| wrap_best(paragraphs))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Angka dan tanda pisah sebagai huruf kecil terangkat: "16" → "¹⁶".
pub fn superscript(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '0' => '\u{2070}',
            '1' => '\u{00b9}',
            '2' => '\u{00b2}',
            '3' => '\u{00b3}',
            '4'..='9' => char::from_u32(0x2074 + (c as u32 - '4' as u32)).unwrap_or(c),
            '-' => '\u{207b}',
            c => c,
        })
        .collect()
}

/// Memotong paragraf menjadi baris dengan lebar yang membuat teks paling
/// besar di layar 16:9, memakai rumus ukuran teks yang sama dengan tampilan
/// slide (`SlideText` di app.slint).
fn wrap_best(paragraphs: &[String]) -> String {
    let mut best: Option<(f32, Vec<String>)> = None;
    for width in (18..=48).step_by(2) {
        let lines: Vec<String> = paragraphs.iter().flat_map(|p| wrap(p, width)).collect();
        let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        // Tinggi 0,84 / (baris × 1,32); lebar 0,88 × 16/9 / (huruf × 0,55);
        // paling besar 1/6,5 tinggi layar.
        let size = (0.84 / (lines.len().max(1) as f32 * 1.32))
            .min(0.88 * 16.0 / 9.0 / (longest.max(10) as f32 * 0.55))
            .min(1.0 / 6.5);
        if best.as_ref().is_none_or(|(b, _)| size >= *b) {
            best = Some((size, lines));
        }
    }
    best.map(|(_, lines)| lines.join("\n")).unwrap_or_default()
}

/// Memotong satu paragraf di spasi menjadi baris paling panjang `width`
/// huruf. Kata yang lebih panjang tetap utuh.
fn wrap(paragraph: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut length = 0;
    for word in paragraph.split_whitespace() {
        let count = word.chars().count();
        if length > 0 && length + 1 + count > width {
            lines.push(std::mem::take(&mut line));
            length = 0;
        }
        if length > 0 {
            line.push(' ');
            length += 1;
        }
        line.push_str(word);
        length += count;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::split_slides;

    fn verse(chapter: u16, number: u16, text: &str) -> Verse {
        Verse {
            book: 43,
            chapter,
            verse: number,
            last: number,
            text: text.into(),
        }
    }

    #[test]
    fn superscript_numbers() {
        assert_eq!(superscript("16"), "¹⁶");
        assert_eq!(superscript("5-6"), "⁵⁻⁶");
        assert_eq!(superscript("1234567890"), "¹²³⁴⁵⁶⁷⁸⁹⁰");
    }

    #[test]
    fn auto_packs_short_verses_and_breaks_at_chapters() {
        let verses = [
            verse(3, 35, "Pendek satu."),
            verse(3, 36, "Pendek dua."),
            verse(4, 1, "Pasal baru."),
            verse(4, 2, "Dua."),
            verse(4, 3, "Tiga."),
            verse(4, 4, "Empat."),
        ];
        let slides = split_slides(&passage_text(&verses, Packing::Auto));
        assert_eq!(
            slides,
            [
                "³⁵Pendek satu.\n³⁶Pendek dua.",
                "¹Pasal baru.\n²Dua.\n³Tiga.",
                "⁴Empat."
            ]
        );
        let one = split_slides(&passage_text(&verses, Packing::One));
        assert_eq!(one.len(), 6);
    }

    #[test]
    fn long_verses_get_their_own_wrapped_slide() {
        let long = "kata ".repeat(60);
        let verses = [
            verse(1, 1, "Awal."),
            verse(1, 2, long.trim()),
            verse(1, 3, "Akhir."),
        ];
        let slides = split_slides(&passage_text(&verses, Packing::Auto));
        assert_eq!(slides.len(), 3);
        let lines: Vec<_> = slides[1].lines().collect();
        assert!(lines.len() > 3, "{lines:?}");
        assert!(lines.iter().all(|l| l.chars().count() <= 48));
        assert_eq!(slides[1].split_whitespace().count(), 60);
    }

    #[test]
    fn wrapping_keeps_every_word() {
        assert_eq!(wrap("satu dua tiga", 8), ["satu dua", "tiga"]);
        assert_eq!(wrap("panjangsekali x", 4), ["panjangsekali", "x"]);
        assert!(wrap("  ", 10).is_empty());
    }
}
