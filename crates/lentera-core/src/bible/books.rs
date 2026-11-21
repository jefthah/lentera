//! Daftar 66 kitab dan pencarian nama kitab.
//!
//! Nomor kitab mengikuti urutan Alkitab Protestan (1 Kejadian … 66 Wahyu),
//! sama dengan nomor `bnumber` di berkas Zefania XML. Nama Indonesia dan
//! singkatannya mengikuti Terjemahan Baru; nama Inggris dikenali supaya
//! berkas dan referensi berbahasa Inggris juga bisa dipakai.

/// Satu kitab.
#[derive(Debug, PartialEq, Eq)]
pub struct Book {
    /// 1–66.
    pub number: u8,
    /// Nama yang ditampilkan, misalnya "Kisah Para Rasul".
    pub name: &'static str,
    /// Singkatan, misalnya "Kis".
    pub short: &'static str,
    /// Nama Inggris, misalnya "Acts".
    pub english: &'static str,
    /// Singkatan lain yang juga dikenali (Indonesia dan Inggris).
    pub aliases: &'static [&'static str],
}

macro_rules! books {
    ($($number:literal $name:literal $short:literal $english:literal [$($alias:literal),*];)*) => {
        /// Ke-66 kitab, urut dari Kejadian sampai Wahyu.
        pub const BOOKS: &[Book] = &[
            $(Book { number: $number, name: $name, short: $short, english: $english, aliases: &[$($alias),*] },)*
        ];
    };
}

books! {
    1 "Kejadian" "Kej" "Genesis" ["Gen", "Gn"];
    2 "Keluaran" "Kel" "Exodus" ["Kel", "Ex", "Exo", "Exod"];
    3 "Imamat" "Im" "Leviticus" ["Lev", "Lv"];
    4 "Bilangan" "Bil" "Numbers" ["Num", "Nm"];
    5 "Ulangan" "Ul" "Deuteronomy" ["Deut", "Dt"];
    6 "Yosua" "Yos" "Joshua" ["Josh", "Jos"];
    7 "Hakim-hakim" "Hak" "Judges" ["Hakim", "Judg", "Jdg"];
    8 "Rut" "Rut" "Ruth" ["Rth"];
    9 "1 Samuel" "1Sam" "1 Samuel" ["1Sa", "1Sm"];
    10 "2 Samuel" "2Sam" "2 Samuel" ["2Sa", "2Sm"];
    11 "1 Raja-raja" "1Raj" "1 Kings" ["1Raja", "1Kgs", "1Ki"];
    12 "2 Raja-raja" "2Raj" "2 Kings" ["2Raja", "2Kgs", "2Ki"];
    13 "1 Tawarikh" "1Taw" "1 Chronicles" ["1Chr", "1Ch", "1Chron"];
    14 "2 Tawarikh" "2Taw" "2 Chronicles" ["2Chr", "2Ch", "2Chron"];
    15 "Ezra" "Ezr" "Ezra" [];
    16 "Nehemia" "Neh" "Nehemiah" [];
    17 "Ester" "Est" "Esther" ["Esth"];
    18 "Ayub" "Ayb" "Job" ["Jb"];
    19 "Mazmur" "Mzm" "Psalms" ["Maz", "Mz", "Ps", "Psa", "Psalm", "Pss"];
    20 "Amsal" "Ams" "Proverbs" ["Prov", "Prv"];
    21 "Pengkhotbah" "Pkh" "Ecclesiastes" ["Eccl", "Ecc", "Qoh"];
    22 "Kidung Agung" "Kid" "Song of Solomon" ["Song", "SoS", "Song of Songs", "Cant"];
    23 "Yesaya" "Yes" "Isaiah" ["Isa"];
    24 "Yeremia" "Yer" "Jeremiah" ["Jer"];
    25 "Ratapan" "Rat" "Lamentations" ["Lam"];
    26 "Yehezkiel" "Yeh" "Ezekiel" ["Ezek", "Ezk"];
    27 "Daniel" "Dan" "Daniel" ["Dn"];
    28 "Hosea" "Hos" "Hosea" [];
    29 "Yoel" "Yl" "Joel" ["Jl"];
    30 "Amos" "Am" "Amos" [];
    31 "Obaja" "Ob" "Obadiah" ["Obad"];
    32 "Yunus" "Yun" "Jonah" ["Jon", "Jnh"];
    33 "Mikha" "Mi" "Micah" ["Mic"];
    34 "Nahum" "Nah" "Nahum" [];
    35 "Habakuk" "Hab" "Habakkuk" [];
    36 "Zefanya" "Zef" "Zephaniah" ["Zeph", "Zep"];
    37 "Hagai" "Hag" "Haggai" [];
    38 "Zakharia" "Za" "Zechariah" ["Zak", "Zech", "Zec"];
    39 "Maleakhi" "Mal" "Malachi" [];
    40 "Matius" "Mat" "Matthew" ["Matt", "Mt"];
    41 "Markus" "Mrk" "Mark" ["Mk", "Mar"];
    42 "Lukas" "Luk" "Luke" ["Lk"];
    43 "Yohanes" "Yoh" "John" ["Jn", "Jhn"];
    44 "Kisah Para Rasul" "Kis" "Acts" ["KPR", "Kisah", "Kisah Rasul", "Ac"];
    45 "Roma" "Rm" "Romans" ["Rom", "Ro"];
    46 "1 Korintus" "1Kor" "1 Corinthians" ["1Co", "1Cor"];
    47 "2 Korintus" "2Kor" "2 Corinthians" ["2Co", "2Cor"];
    48 "Galatia" "Gal" "Galatians" [];
    49 "Efesus" "Ef" "Ephesians" ["Eph"];
    50 "Filipi" "Flp" "Philippians" ["Fil", "Phil", "Php"];
    51 "Kolose" "Kol" "Colossians" ["Col"];
    52 "1 Tesalonika" "1Tes" "1 Thessalonians" ["1Th", "1Thess"];
    53 "2 Tesalonika" "2Tes" "2 Thessalonians" ["2Th", "2Thess"];
    54 "1 Timotius" "1Tim" "1 Timothy" ["1Ti"];
    55 "2 Timotius" "2Tim" "2 Timothy" ["2Ti"];
    56 "Titus" "Tit" "Titus" [];
    57 "Filemon" "Flm" "Philemon" ["Phlm", "Philem"];
    58 "Ibrani" "Ibr" "Hebrews" ["Heb"];
    59 "Yakobus" "Yak" "James" ["Jas", "Jm"];
    60 "1 Petrus" "1Ptr" "1 Peter" ["1Pet", "1Pt", "1Pe"];
    61 "2 Petrus" "2Ptr" "2 Peter" ["2Pet", "2Pt", "2Pe"];
    62 "1 Yohanes" "1Yoh" "1 John" ["1Jn", "1Jo"];
    63 "2 Yohanes" "2Yoh" "2 John" ["2Jn", "2Jo"];
    64 "3 Yohanes" "3Yoh" "3 John" ["3Jn", "3Jo"];
    65 "Yudas" "Yud" "Jude" [];
    66 "Wahyu" "Why" "Revelation" ["Wah", "Rev", "Re", "Revelations"];
}

/// Kitab bernomor `number` (1–66).
pub fn book(number: u8) -> Option<&'static Book> {
    BOOKS.get(usize::from(number).checked_sub(1)?)
}

/// Bentuk baku nama kitab untuk dibandingkan: huruf kecil, tanpa spasi,
/// titik, dan tanda hubung. Angka Romawi di depan (I, II, III) menjadi angka
/// biasa, jadi "II Korintus", "2 Kor." dan "2kor" sama-sama menjadi "2kor".
pub(crate) fn normalize(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let mut rest = lower.as_str();
    let mut out = String::new();
    for (roman, digit) in [("iii", '3'), ("ii", '2'), ("i", '1')] {
        if let Some(after) = rest.strip_prefix(roman)
            && after.starts_with([' ', '.'])
        {
            out.push(digit);
            rest = after;
            break;
        }
    }
    out.extend(rest.chars().filter(|c| c.is_alphanumeric()));
    out
}

/// Hasil [`find_book`].
#[derive(Debug, PartialEq, Eq)]
pub enum BookMatch {
    Found(&'static Book),
    /// Nama cocok dengan awal beberapa kitab.
    Ambiguous(Vec<&'static Book>),
    NotFound,
}

/// Mencari kitab dari nama, singkatan, atau awal nama. Huruf besar/kecil,
/// spasi, dan titik diabaikan.
pub fn find_book(name: &str) -> BookMatch {
    let key = normalize(name);
    if key.is_empty() {
        return BookMatch::NotFound;
    }
    let names = |book: &'static Book| {
        [book.name, book.short, book.english]
            .into_iter()
            .chain(book.aliases.iter().copied())
            .map(normalize)
    };
    if let Some(book) = BOOKS.iter().find(|book| names(book).any(|n| n == key)) {
        return BookMatch::Found(book);
    }
    // Awal nama, hanya dari nama lengkap supaya "Ma" tidak cocok dengan
    // singkatan yang kebetulan berawalan sama.
    let found: Vec<_> = BOOKS
        .iter()
        .filter(|book| {
            [book.name, book.english]
                .into_iter()
                .any(|n| normalize(n).starts_with(&key))
        })
        .collect();
    match found.as_slice() {
        [] => BookMatch::NotFound,
        [book] => BookMatch::Found(book),
        _ => BookMatch::Ambiguous(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(name: &str) -> Option<u8> {
        match find_book(name) {
            BookMatch::Found(book) => Some(book.number),
            _ => None,
        }
    }

    #[test]
    fn table_is_complete_and_names_are_unique() {
        assert_eq!(BOOKS.len(), 66);
        let mut seen = std::collections::HashMap::new();
        for (index, book) in BOOKS.iter().enumerate() {
            assert_eq!(usize::from(book.number), index + 1);
            for name in [book.name, book.short, book.english]
                .into_iter()
                .chain(book.aliases.iter().copied())
            {
                let key = normalize(name);
                if let Some(other) = seen.insert(key.clone(), book.number) {
                    assert_eq!(other, book.number, "{name} dipakai dua kitab");
                }
            }
        }
    }

    #[test]
    fn indonesian_and_english_names_and_abbreviations() {
        assert_eq!(number("Yoh"), Some(43));
        assert_eq!(number("yohanes"), Some(43));
        assert_eq!(number("John"), Some(43));
        assert_eq!(number("1 Yoh"), Some(62));
        assert_eq!(number("1Yoh"), Some(62));
        assert_eq!(number("I Yohanes"), Some(62));
        assert_eq!(number("III Yoh."), Some(64));
        assert_eq!(number("2 Kor."), Some(47));
        assert_eq!(number("Kisah Para Rasul"), Some(44));
        assert_eq!(number("kpr"), Some(44));
        assert_eq!(number("Mzm"), Some(19));
        assert_eq!(number("Psalm"), Some(19));
        assert_eq!(number("Hakim-hakim"), Some(7));
        assert_eq!(number("Why"), Some(66));
        assert_eq!(number("Kid"), Some(22));
        assert_eq!(number("Song of Solomon"), Some(22));
    }

    #[test]
    fn prefixes_must_point_to_one_book() {
        assert_eq!(number("Kejad"), Some(1));
        assert_eq!(number("Wahy"), Some(66));
        assert_eq!(number("Filem"), Some(57));
        assert_eq!(number("Mar"), Some(41));
        // "Fil" adalah singkatan Filipi walau juga awal "Filemon".
        assert_eq!(number("Fil"), Some(50));
        match find_book("Ye") {
            BookMatch::Ambiguous(books) => {
                let names: Vec<_> = books.iter().map(|b| b.name).collect();
                assert_eq!(names, ["Yesaya", "Yeremia", "Yehezkiel"]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(find_book("Xyz"), BookMatch::NotFound);
        assert_eq!(find_book(" "), BookMatch::NotFound);
    }
}
