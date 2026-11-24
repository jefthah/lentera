//! Lirik EasyWorship (RTF) menjadi slide teks polos.
//!
//! Hanya bagian RTF yang dipakai EasyWorship yang dibaca: paragraf (`\par`),
//! baris (`\line`), karakter `\'hh` (Windows-1252) dan `\uN`, serta dua
//! penanda EasyWorship 6 ke atas: `\sdslidemarker` (paragraf pemisah slide)
//! dan `\sdparawysiwghidden` (paragraf label seperti "Verse 1" yang tidak
//! tampil di layar). Format huruf, warna, dan tata letak diabaikan.

/// Satu paragraf RTF.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Paragraph {
    /// Teks paragraf; `\line` menjadi `\n`.
    pub text: String,
    /// Paragraf pemisah slide (`\sdslidemarker`).
    pub slide_marker: bool,
    /// Paragraf yang tidak tampil di layar EasyWorship, biasanya label bagian.
    pub hidden: bool,
}

/// Grup RTF yang isinya bukan teks lirik.
const SKIPPED_GROUPS: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "pict",
    "object",
    "header",
    "footer",
    "headerl",
    "headerr",
    "headerf",
    "footerl",
    "footerr",
    "footerf",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "generator",
    "xmlnstbl",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "filetbl",
    "revtbl",
    "pntext",
    "pntxta",
    "pntxtb",
    "pnseclvl",
    "fldinst",
    "nonshppict",
    "bkmkstart",
    "bkmkend",
    "expandedcolortbl",
    "listtext",
    "shpinst",
    "sp",
    "sn",
    "sv",
    "defchp",
    "defpap",
    "mmathPr",
    "wgrffmtfilter",
    "pgdsctbl",
];

#[derive(Clone, Copy)]
struct Group {
    skip: bool,
    /// Jumlah karakter pengganti setelah `\uN` (`\ucN`).
    unicode_skip: usize,
}

struct Parser {
    paragraphs: Vec<Paragraph>,
    current: Paragraph,
    group: Group,
    stack: Vec<Group>,
    /// Karakter pengganti `\uN` yang masih harus dilewati.
    pending_skip: usize,
    /// Setengah pertama pasangan surrogate `\uN`.
    high_surrogate: Option<u16>,
}

impl Parser {
    fn push(&mut self, c: char) {
        if self.pending_skip > 0 {
            self.pending_skip -= 1;
        } else if !self.group.skip {
            self.text(c);
        }
    }

    /// Menambah satu karakter teks. Karakter kontrol selain tab dibuang:
    /// lirik EasyWorship 7 kadang berisi NUL di ujung baris, yang tidak tampil
    /// di EasyWorship tetapi ditolak server sinkron Lentera.
    fn text(&mut self, c: char) {
        if !c.is_control() || c == '\t' {
            self.current.text.push(c);
        }
    }

    fn end_paragraph(&mut self) {
        self.paragraphs.push(std::mem::take(&mut self.current));
    }

    fn unicode(&mut self, value: i32) {
        // Nilai di atas 32767 ditulis negatif.
        let unit = value.rem_euclid(65536) as u32;
        self.pending_skip = self.group.unicode_skip;
        if self.group.skip {
            return;
        }
        match unit {
            0xD800..=0xDBFF => self.high_surrogate = Some(unit as u16),
            0xDC00..=0xDFFF => {
                if let Some(high) = self.high_surrogate.take() {
                    let c = char::decode_utf16([high, unit as u16])
                        .next()
                        .and_then(Result::ok);
                    self.text(c.unwrap_or('\u{fffd}'));
                }
            }
            _ => self.text(char::from_u32(unit).unwrap_or('\u{fffd}')),
        }
    }

    fn control_word(&mut self, word: &str, param: Option<i32>) {
        if SKIPPED_GROUPS.contains(&word) {
            self.group.skip = true;
            return;
        }
        match word {
            // Penanda paragraf berlaku walaupun grupnya dilewati.
            "sdslidemarker" => self.current.slide_marker = true,
            "sdparawysiwghidden" => self.current.hidden = true,
            "uc" => self.group.unicode_skip = param.unwrap_or(1).max(0) as usize,
            _ if self.group.skip => {}
            "par" | "sect" | "page" | "row" => self.end_paragraph(),
            "line" => self.current.text.push('\n'),
            "tab" | "cell" | "emspace" | "enspace" | "qmspace" => self.current.text.push(' '),
            "u" => {
                if let Some(value) = param {
                    self.unicode(value);
                }
            }
            "emdash" => self.current.text.push('\u{2014}'),
            "endash" => self.current.text.push('\u{2013}'),
            "lquote" => self.current.text.push('\u{2018}'),
            "rquote" => self.current.text.push('\u{2019}'),
            "ldblquote" => self.current.text.push('\u{201c}'),
            "rdblquote" => self.current.text.push('\u{201d}'),
            "bullet" => self.current.text.push('\u{2022}'),
            _ => {}
        }
    }
}

/// Membaca paragraf dari lirik EasyWorship. Lirik yang bukan RTF dibaca
/// sebagai teks biasa, satu paragraf per baris.
pub(super) fn paragraphs(input: &str) -> Vec<Paragraph> {
    if !input.trim_start().starts_with("{\\rtf") {
        return input
            .replace("\r\n", "\n")
            .split(['\n', '\r'])
            .map(|line| Paragraph {
                text: line.to_owned(),
                ..Paragraph::default()
            })
            .collect();
    }

    let mut parser = Parser {
        paragraphs: Vec::new(),
        current: Paragraph::default(),
        group: Group {
            skip: false,
            unicode_skip: 1,
        },
        stack: Vec::new(),
        pending_skip: 0,
        high_surrogate: None,
    };
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '{' => {
                parser.stack.push(parser.group);
                parser.pending_skip = 0;
            }
            '}' => {
                if let Some(group) = parser.stack.pop() {
                    parser.group = group;
                }
                parser.pending_skip = 0;
            }
            '\r' | '\n' => {}
            '\\' => {
                let Some(&next) = chars.get(i) else { break };
                if next.is_ascii_alphabetic() {
                    let start = i;
                    while i < chars.len() && chars[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    let digits = i;
                    if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let param = (i > digits)
                        .then(|| chars[digits..i].iter().collect::<String>().parse().ok())
                        .flatten();
                    if chars.get(i) == Some(&' ') {
                        i += 1;
                    }
                    parser.control_word(&word, param);
                    continue;
                }
                i += 1;
                match next {
                    '\'' => {
                        let hex: String = chars.iter().skip(i).take(2).collect();
                        i += 2;
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            parser.push(super::cp1252(byte));
                        }
                    }
                    '*' => parser.group.skip = true,
                    '\\' | '{' | '}' => parser.push(next),
                    '~' => parser.push(' '),
                    '_' => parser.push('-'),
                    '\r' | '\n' if !parser.group.skip => parser.end_paragraph(),
                    _ => {}
                }
            }
            _ => parser.push(c),
        }
    }
    if !parser.current.text.is_empty() {
        parser.end_paragraph();
    }
    parser.paragraphs
}

/// Menyusun slide dari paragraf. Bila ada `\sdslidemarker` (EasyWorship 6
/// ke atas), hanya penanda itu yang memisahkan slide dan baris kosong di
/// dalam slide dibuang, karena di Lentera baris kosong berarti slide baru.
/// Tanpa penanda (EasyWorship 2009), paragraf kosong memisahkan slide.
/// Label bagian ("Verse 1", "Reff:") di baris pertama slide dibuang.
pub(super) fn slides(paragraphs: &[Paragraph]) -> Vec<String> {
    let markers = paragraphs.iter().any(|p| p.slide_marker);
    let mut slides = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut close = |lines: &mut Vec<String>| {
        if lines.first().is_some_and(|line| is_label(line)) {
            lines.remove(0);
        }
        if !lines.is_empty() {
            slides.push(lines.join("\n"));
        }
        lines.clear();
    };
    for paragraph in paragraphs {
        if paragraph.slide_marker {
            close(&mut lines);
        }
        // Paragraf tersembunyi biasanya label ("Verse 1") atau catatan
        // ("end"), tetapi kadang baris lirik yang salah format; lirik tetap
        // diambil.
        if paragraph.hidden && (paragraph.text.trim().is_empty() || is_label(&paragraph.text)) {
            continue;
        }
        if paragraph.text.trim().is_empty() {
            if !markers {
                close(&mut lines);
            }
            continue;
        }
        lines.extend(
            paragraph
                .text
                .split('\n')
                .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|line| !line.is_empty()),
        );
    }
    close(&mut lines);
    slides
}

/// Label bagian lagu yang ditulis sebagai baris sendiri, misalnya "Verse 1",
/// "Chorus", "Reff:", atau "(Bridge)", juga penanda akhir "End".
pub(super) fn is_label(line: &str) -> bool {
    let line = line
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .trim_end_matches([':', '.'])
        .trim_end_matches(|c: char| c.is_ascii_digit() || c.is_whitespace());
    let word: String = line
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();
    // Huruf saja yang dihitung, jadi "Pre-Chorus" dan "Pre Chorus" sama.
    if word.chars().count() + 4 < line.chars().count() {
        return false;
    }
    const LABELS: &[&str] = &[
        "verse",
        "chorus",
        "bridge",
        "prechorus",
        "prereff",
        "preref",
        "tag",
        "intro",
        "outro",
        "end",
        "ending",
        "coda",
        "interlude",
        "interlud",
        "refrain",
        "reff",
        "ref",
        "bait",
        "ayat",
        "musik",
        "music",
        "instrumental",
    ];
    LABELS.contains(&word.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slides_of(rtf: &str) -> Vec<String> {
        slides(&paragraphs(rtf))
    }

    #[test]
    fn easyworship_7_markers_hidden_labels_and_unicode() {
        let rtf = concat!(
            r"{\rtf1\ansi\deff0\sdeasyworship2",
            "\r\n",
            r"{\fonttbl{\f0 Tahoma;}}",
            "\r\n",
            r"{\colortbl ;}",
            "\r\n",
            r"{\pard\sdparawysiwghidden\qc\qdef\plain\fs40{\*\sdfsreal 20}\sdfsauto Verse\par}",
            "\r\n",
            r"{\pard\qc\plain\fs40{\*\sdfsreal 20}\sdfsauto Aku hendak bersyukur\par}",
            "\r\n",
            r"{\pard\qc\plain\fs40\sdfsauto\par}",
            "\r\n",
            r"{\pard\qc\plain\fs40\sdfsauto Kar'na keadilan-Nya\par}",
            "\r\n",
            r"{\pard\sdslidemarker\qc\plain\fs40\sdfsauto\par}",
            "\r\n",
            r"{\pard\sdparawysiwghidden\qc\plain Chorus\par}",
            "\r\n",
            // Lirik yang ikut tersembunyi tetap diambil.
            r"{\pard\sdparawysiwghidden\qc\plain Kau setia\par}",
            "\r\n",
            // Karakter 8230 (U+2026) adalah "…".
            r"{\pard\qc\plain \u",
            "8230",
            r"? Karena kita\line Dia   menderita\par}",
            "\r\n",
            r"{\pard\qc\plain caf\u233?\par}",
            "}"
        );
        assert_eq!(
            slides_of(rtf),
            vec![
                "Aku hendak bersyukur\nKar'na keadilan-Nya",
                "Kau setia\n\u{2026} Karena kita\nDia menderita\ncaf\u{e9}"
            ]
        );
    }

    #[test]
    fn easyworship_2009_blank_paragraphs_split_slides() {
        let rtf = concat!(
            r"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\fnil Tahoma;}}",
            "\r\n",
            r"\viewkind4\uc1\pard\f0\fs20 Reff:\par",
            "\r\n",
            r"Kau Tuhan \'e9 \{x\}\par",
            "\r\n",
            r"\par",
            "\r\n",
            r"\par",
            "\r\n",
            r"(Verse 2)\par",
            "\r\n",
            r"\par",
            "\r\n",
            r"Bait kedua\par",
            "\r\n",
            r"Ulang 2x\par",
            "\r\n",
            "}"
        );
        assert_eq!(
            slides_of(rtf),
            vec!["Kau Tuhan \u{e9} {x}", "Bait kedua\nUlang 2x"]
        );
    }

    #[test]
    fn plain_text_lyrics() {
        assert_eq!(
            slides_of("Chorus\r\nBaris satu\r\n\r\n\r\nBaris dua\r\n"),
            vec!["Baris satu", "Baris dua"]
        );
        assert!(slides_of("").is_empty());
        assert!(slides_of(r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\par\par}").is_empty());
    }

    #[test]
    fn control_characters_are_dropped() {
        // NUL ditulis langsung, sebagai \'00, atau sebagai \u0.
        let rtf = "{\\rtf1\\ansi Bait satu\0\\'00\\par Bait\\u0 ? dua\\par}";
        assert_eq!(slides_of(rtf), vec!["Bait satu\nBait dua"]);
        // Paragraf yang hanya berisi NUL tampak kosong di EasyWorship, jadi
        // diperlakukan seperti paragraf kosong (pemisah slide EasyWorship 2009).
        let rtf = "{\\rtf1\\ansi A\\par\0\\par B\\par}";
        assert_eq!(slides_of(rtf), vec!["A", "B"]);
    }

    #[test]
    fn surrogate_pairs_and_skipped_fallbacks() {
        let rtf = r"{\rtf1\ansi\uc2 a\u-10179\'3f\'3f\u-8704\'3f\'3fb\par}";
        assert_eq!(slides_of(rtf), vec!["a\u{1f600}b"]);
    }

    #[test]
    fn labels() {
        for label in [
            "Verse",
            "verse 1",
            "CHORUS",
            "Reff:",
            "Reff :",
            "(Bridge)",
            "Pre-Chorus",
            "pre chorus 2",
            "Bait 3.",
            "Interlude",
            "End",
            "end 1",
            "Verse2",
        ] {
            assert!(is_label(label), "{label}");
        }
        for line in [
            "Chorus 2x",
            "Kasih-Mu Tuhan",
            "Refrain dari surga",
            "Verse satu dua",
            "Endless",
            "",
            "123",
        ] {
            assert!(!is_label(line), "{line}");
        }
    }
}
