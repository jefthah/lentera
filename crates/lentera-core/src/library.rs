use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    #[default]
    Song,
    Verse,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub kind: Kind,
    pub title: String,
    pub content: String,
    /// Gambar latar khusus materi ini, dari folder latar profil (lihat
    /// [`crate::background`]). `None` berarti mengikuti latar umum
    /// ([`Library::background`] dan [`Library::background_image`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}

impl Item {
    pub fn slides(&self) -> Vec<String> {
        split_slides(&self.content)
    }
    /// Sama dengan `slides().len()`, tetapi tanpa menyalin teks. Dipakai
    /// untuk setiap baris daftar pustaka.
    pub fn slide_count(&self) -> usize {
        count_slides(&self.content)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub version: u32,
    pub next_id: u64,
    pub items: Vec<Item>,
    pub service: Vec<u64>,
    /// Warna latar bawaan (lihat `Theme.slide-bg` di tampilan).
    pub background: usize,
    /// Gambar latar dari folder latar profil (lihat [`crate::background`]).
    /// Bila ada, gambar ini dipakai dan `background` diabaikan.
    pub background_image: Option<String>,
    pub monitor: usize,
    /// Materi sementara yang tidak disimpan, misalnya ayat Alkitab yang sedang
    /// dilihat. Lihat [`Library::set_scratch`].
    #[serde(skip)]
    pub scratch: Vec<Item>,
}

/// Id materi sementara dimulai dari angka ini, jauh di atas id materi biasa,
/// dan masih muat di `i32` untuk tampilan.
pub const SCRATCH_ID_START: u64 = 1 << 30;

impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1, next_id: 3,
            items: vec![
                Item { id: 1, kind: Kind::Song, title: "Selamat datang di Lentera".into(),
                    content: "Siapkan hati untuk beribadah\nBersama memuji dan bersyukur\n\nKasih Tuhan menyertai kita\nHari ini dan selamanya".into(),
                    background: None },
                Item { id: 2, kind: Kind::Verse, title: "Ayat contoh — ganti dengan referensi".into(),
                    content: "Masukkan teks ayat dari terjemahan\nyang Anda berhak gunakan.\n\nAnda juga bisa mengimpor ayat\ndari berkas TSV UTF-8.".into(),
                    background: None },
            ],
            service: vec![1, 2], background: 0, background_image: None, monitor: 0,
            scratch: Vec::new(),
        }
    }
}

impl Library {
    /// Pustaka tanpa materi, untuk profil baru.
    pub fn blank() -> Self {
        Self {
            next_id: 1,
            items: Vec::new(),
            service: Vec::new(),
            ..Self::default()
        }
    }
}

/// Karakter kontrol yang tidak punya arti di lirik atau ayat, misalnya NUL
/// sisa format EasyWorship. Baris baru dan tab tetap dipakai.
fn is_stray_control(c: char) -> bool {
    c.is_control() && !matches!(c, '\n' | '\r' | '\t')
}

/// Teks tanpa karakter kontrol liar. Server sinkron menolak materi berisi
/// NUL, dan karakter kontrol lain juga tidak tampil di layar.
pub fn clean_text(text: &str) -> String {
    if text.contains(is_stray_control) {
        text.chars().filter(|&c| !is_stray_control(c)).collect()
    } else {
        text.to_owned()
    }
}

pub fn split_slides(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut slides = Vec::new();
    let mut lines = Vec::new();
    for line in normalized.lines() {
        if line.trim().is_empty() {
            if !lines.is_empty() {
                slides.push(lines.join("\n"));
                lines.clear();
            }
        } else {
            lines.push(line.trim_end().to_owned());
        }
    }
    if !lines.is_empty() {
        slides.push(lines.join("\n"));
    }
    slides
}

/// Jumlah slide menurut aturan [`split_slides`]: kelompok baris berisi yang
/// dipisah baris kosong. `\r\n` dan `\r` dihitung sebagai satu ganti baris.
/// Dibaca per byte tanpa menyalin teks, karena dipanggil untuk setiap baris
/// daftar pustaka setiap kali pencarian berubah.
fn count_slides(text: &str) -> usize {
    let bytes = text.as_bytes();
    let (mut count, mut previous, mut start) = (0, false, 0);
    while start <= bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'\n' || b == b'\r')
            .map_or(bytes.len(), |i| start + i);
        // `\r` dan `\n` tidak pernah muncul di dalam huruf UTF-8 lain, jadi
        // irisan ini selalu tepat di batas huruf.
        let filled = !text[start..end].trim().is_empty();
        if filled && !previous {
            count += 1;
        }
        previous = filled;
        let line_break = if bytes[end..].starts_with(b"\r\n") {
            2
        } else {
            1
        };
        start = end + line_break;
    }
    count
}

impl Library {
    /// Materi biasa atau materi sementara dengan id ini.
    pub fn item(&self, id: u64) -> Option<&Item> {
        self.items.iter().chain(&self.scratch).find(|i| i.id == id)
    }
    /// Menambah materi sementara: bisa dipratinjau dan ditayangkan seperti
    /// materi biasa, tetapi tidak tampil di pustaka dan tidak disimpan.
    /// Materi sementara lain dibuang, kecuali `keep` (misalnya yang sedang
    /// tayang).
    pub fn set_scratch(
        &mut self,
        kind: Kind,
        title: String,
        content: String,
        keep: Option<u64>,
    ) -> u64 {
        // Id baru selalu lebih besar dari id sebelumnya, supaya id yang sudah
        // dibuang tidak menunjuk materi lain.
        let id = self
            .scratch
            .iter()
            .map(|i| i.id + 1)
            .max()
            .unwrap_or(SCRATCH_ID_START);
        self.scratch.retain(|i| Some(i.id) == keep);
        self.scratch.push(Item {
            id,
            kind,
            title,
            content,
            background: None,
        });
        id
    }
    pub fn is_scratch(id: u64) -> bool {
        id >= SCRATCH_ID_START
    }
    pub fn add(&mut self, kind: Kind, title: String, content: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Item {
            id,
            kind,
            title: clean_text(&title),
            content: clean_text(&content),
            background: None,
        });
        id
    }
    /// Membuang karakter kontrol liar dari judul dan isi semua materi, misalnya
    /// NUL di lagu yang diimpor dari EasyWorship oleh Lentera 0.6. Hasilnya
    /// jumlah materi yang berubah.
    pub fn clean_items(&mut self) -> usize {
        let mut changed = 0;
        for item in &mut self.items {
            if item.title.contains(is_stray_control) || item.content.contains(is_stray_control) {
                item.title = clean_text(&item.title);
                item.content = clean_text(&item.content);
                changed += 1;
            }
        }
        changed
    }
    /// Gambar latar khusus satu materi pustaka; `None` kembali ke latar umum.
    /// Materi sementara tidak punya latar sendiri. `false` bila materinya
    /// tidak ada atau latarnya sudah sama.
    pub fn set_item_background(&mut self, id: u64, name: Option<String>) -> bool {
        match self.items.iter_mut().find(|i| i.id == id) {
            Some(item) if item.background != name => {
                item.background = name;
                true
            }
            _ => false,
        }
    }
    /// Jumlah materi yang memakai gambar latar `name` sebagai latar sendiri.
    pub fn background_users(&self, name: &str) -> usize {
        self.items
            .iter()
            .filter(|i| i.background.as_deref() == Some(name))
            .count()
    }
    /// Materi yang memakai gambar `name` kembali ke latar umum, misalnya
    /// karena gambarnya dihapus. Hasilnya jumlah materi yang berubah.
    pub fn forget_item_background(&mut self, name: &str) -> usize {
        let mut changed = 0;
        for item in &mut self.items {
            if item.background.as_deref() == Some(name) {
                item.background = None;
                changed += 1;
            }
        }
        changed
    }
    pub fn remove(&mut self, id: u64) {
        self.items.retain(|i| i.id != id);
        self.scratch.retain(|i| i.id != id);
        self.service.retain(|i| *i != id);
    }
    /// Menyimpan isi editor ke materi. Judul kosong menjadi "Tanpa judul".
    pub fn update(&mut self, id: u64, title: &str, content: &str) -> bool {
        let Some(item) = self.items.iter_mut().find(|i| i.id == id) else {
            return false;
        };
        let title = clean_text(title);
        let title = title.trim();
        item.title = if title.is_empty() {
            "Tanpa judul".into()
        } else {
            title.into()
        };
        item.content = clean_text(&content.replace("\r\n", "\n"));
        true
    }
    /// Membaca `library.json` Lentera ≤ 0.2.x. Bila berkas tidak ada, hasilnya
    /// pustaka contoh. Penyimpanan sekarang memakai [`Store`](crate::Store).
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path).map_err(|e| format!("Tidak bisa membaca data: {e}"))?;
        let data: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Data tidak valid. Berkas asli dipertahankan.\n{e}"))?;
        data.validate()?;
        Ok(data)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Versi berkas belum didukung.".into());
        }
        let mut ids = std::collections::HashSet::new();
        for item in &self.items {
            if item.id == 0 || !ids.insert(item.id) || item.id >= self.next_id {
                return Err("ID di pustaka tidak valid; berkas asli dipertahankan.".into());
            }
        }
        if self.service.iter().any(|id| !ids.contains(id)) {
            return Err("Susunan mengacu ke materi yang hilang.".into());
        }
        Ok(())
    }
    /// Parse the entire TSV before mutating the library, so malformed imports are atomic.
    pub fn import_verses(&mut self, input: &str) -> Result<usize, String> {
        let mut rows = Vec::new();
        for (index, line) in input.trim_start_matches('\u{feff}').lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((reference, text)) = line.split_once('\t') else {
                return Err(format!(
                    "Baris {} harus berisi referensi, TAB, lalu teks ayat.",
                    index + 1
                ));
            };
            if reference.trim().is_empty() || text.trim().is_empty() {
                return Err(format!(
                    "Referensi atau teks kosong di baris {}.",
                    index + 1
                ));
            }
            rows.push((
                reference.trim().to_owned(),
                text.trim().replace("\\n", "\n"),
            ));
        }
        if rows.is_empty() {
            return Err("Tidak ada ayat di berkas ini.".into());
        }
        let mut known: std::collections::HashSet<_> = self
            .items
            .iter()
            .filter(|i| i.kind == Kind::Verse)
            .map(|i| (i.title.clone(), i.content.clone()))
            .collect();
        let mut added = 0;
        for (reference, text) in rows {
            if known.insert((reference.clone(), text.clone())) {
                self.add(Kind::Verse, reference, text);
                added += 1;
            }
        }
        Ok(added)
    }
    /// Semua materi yang cocok, urut seperti di pustaka. Tidak dibatasi:
    /// daftar Slint hanya membuat baris yang terlihat (lihat
    /// `docs/PENGUJIAN.md`, "Pustaka lebih dari 500 materi").
    pub fn search(&self, kind: Option<Kind>, query: &str) -> Vec<&Item> {
        let query = query.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                kind.is_none_or(|kind| item.kind == kind)
                    && (query.is_empty()
                        || item.title.to_lowercase().contains(&query)
                        || item.content.to_lowercase().contains(&query))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stray_control_characters_are_removed() {
        assert_eq!(
            clean_text("Kasih\0-Mu\u{1b}\r\n\tTuhan 🙏\u{85}\0"),
            "Kasih-Mu\r\n\tTuhan 🙏"
        );
        let mut library = Library::blank();
        let id = library.add(Kind::Song, "Ju\0dul".into(), "Bait\0 satu".into());
        assert_eq!(library.item(id).unwrap().title, "Judul");
        assert_eq!(library.item(id).unwrap().content, "Bait satu");
        assert!(library.update(id, "\0 Baru \0", "A\0\r\nB"));
        assert_eq!(library.item(id).unwrap().title, "Baru");
        assert_eq!(library.item(id).unwrap().content, "A\nB");

        // Data lama yang sudah berisi NUL.
        library.items[0].content.push('\0');
        let clean = library.add(Kind::Verse, "Ayat".into(), "Teks".into());
        assert_eq!(library.clean_items(), 1);
        assert_eq!(library.item(id).unwrap().content, "A\nB");
        assert_eq!(library.item(clean).unwrap().content, "Teks");
        assert_eq!(library.clean_items(), 0);
    }
    #[test]
    fn slides_keep_unicode_and_ignore_repeated_blank_lines() {
        assert_eq!(
            split_slides("  \r\nKasih-Mu\r\nTuhan 🙏\r\n \r\n\r\nAmin\r\n"),
            vec!["Kasih-Mu\nTuhan 🙏", "Amin"]
        );
        assert!(split_slides(" \n\n").is_empty());
    }
    #[test]
    fn slide_count_matches_split_slides() {
        for text in [
            "",
            " \n\n",
            "Satu",
            "Satu\nDua\n\nTiga\n",
            "  \r\nKasih-Mu\r\nTuhan 🙏\r\n \r\n\r\nAmin\r\n",
            "A\rB\r\rC",
            "A\r\r\nB\n\rC",
            "A\n\u{3000}\nB\n\t\n\nC D\n",
            "Akhir\r",
            "é\r\n\r\nü\r\nñ",
        ] {
            let item = Item {
                id: 1,
                kind: Kind::Song,
                title: String::new(),
                content: text.into(),
                background: None,
            };
            assert_eq!(item.slide_count(), item.slides().len(), "{text:?}");
        }
    }
    #[test]
    fn malformed_import_never_partially_changes_library() {
        let mut library = Library::default();
        let before = library.items.len();
        assert!(
            library
                .import_verses("Contoh 1:1\tTeks\nbaris rusak")
                .is_err()
        );
        assert_eq!(library.items.len(), before);
    }
    #[test]
    fn imports_deduplicate_and_allow_multiple_translations() {
        let mut library = Library::default();
        assert_eq!(
            library
                .import_verses("\u{feff}Contoh 1:1\tTeks\nContoh 1:1\tTeks\n")
                .unwrap(),
            1
        );
        assert_eq!(library.import_verses("Contoh 1:1\tTeks").unwrap(), 0);
        assert_eq!(
            library
                .import_verses("Contoh 1:1\tTerjemahan lain")
                .unwrap(),
            1
        );
    }
    #[test]
    fn delete_also_removes_all_service_references() {
        let mut library = Library::default();
        library.service.push(1);
        library.remove(1);
        assert!(library.service.iter().all(|id| *id != 1));
        library.validate().unwrap();
    }
    #[test]
    fn search_is_case_insensitive_and_includes_content() {
        let library = Library::default();
        assert_eq!(
            library
                .search(Some(Kind::Song), "BERSYUKUR")
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert!(library.search(Some(Kind::Verse), "BERSYUKUR").is_empty());
    }
    #[test]
    fn search_returns_every_match_in_library_order() {
        // Dulu hasil dipotong di 500; impor EasyWorship bisa ratusan lagu.
        let mut library = Library::blank();
        for n in 1..=1200 {
            library.add(Kind::Song, format!("Lagu {n}"), format!("Bait {n}"));
        }
        library.add(Kind::Verse, "Mazmur 23:1".into(), "Tuhan gembalaku".into());
        let all = library.search(None, "");
        assert_eq!(all.len(), 1201);
        assert!(all.windows(2).all(|w| w[0].id < w[1].id));
        assert_eq!(all[1200].title, "Mazmur 23:1");
        assert_eq!(library.search(Some(Kind::Song), "").len(), 1200);
        assert_eq!(
            library
                .search(None, "lagu 1100")
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>(),
            vec![1100]
        );
    }
    #[test]
    fn load_reads_old_files_and_keeps_corrupt_ones() {
        let directory = std::env::temp_dir().join(format!("lentera-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("library.json");
        assert_eq!(Library::load(&path).unwrap(), Library::default());
        let mut library = Library::default();
        library.items[0].title = "Judul baru".into();
        fs::write(&path, serde_json::to_vec(&library).unwrap()).unwrap();
        assert_eq!(Library::load(&path).unwrap(), library);
        fs::write(&path, b"broken").unwrap();
        assert!(Library::load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn editor_update_cleans_title_and_line_endings() {
        let mut library = Library::default();
        assert!(library.update(1, "  ", "A\r\nB"));
        assert_eq!(library.items[0].title, "Tanpa judul");
        assert_eq!(library.items[0].content, "A\nB");
        assert!(library.update(1, " Judul ", ""));
        assert_eq!(library.items[0].title, "Judul");
        assert!(!library.update(99, "x", "y"));
    }
    #[test]
    fn scratch_items_show_like_items_but_stay_out_of_the_library() {
        let mut library = Library::default();
        let first = library.set_scratch(
            Kind::Verse,
            "Yohanes 3:16 (TB)".into(),
            "A

B"
            .into(),
            None,
        );
        assert!(Library::is_scratch(first) && !Library::is_scratch(1));
        assert_eq!(library.item(first).unwrap().slides(), ["A", "B"]);
        assert_eq!(library.slide(first, 1).unwrap().text, "B");
        assert!(library.search(None, "Yohanes").is_empty());
        library.validate().unwrap();
        // Yang lama dibuang, kecuali yang diminta tetap ada.
        let second = library.set_scratch(Kind::Verse, "B".into(), "B".into(), Some(first));
        let third = library.set_scratch(Kind::Verse, "C".into(), "C".into(), Some(first));
        assert!(library.item(first).is_some() && library.item(second).is_none());
        assert_ne!(third, first);
        library.set_scratch(Kind::Verse, "D".into(), "D".into(), None);
        assert_eq!(library.scratch.len(), 1);
        assert!(library.item(first).is_none());
    }
    #[test]
    fn invalid_ids_and_future_formats_are_rejected() {
        let mut library = Library::default();
        library.items[1].id = 1;
        assert!(library.validate().is_err());
        library = Library::default();
        library.version = 99;
        assert!(library.validate().is_err());
    }
    #[test]
    fn items_can_have_their_own_background() {
        let mut library = Library::default();
        let salib = Some("Salib.jpg".to_owned());
        assert!(library.set_item_background(1, salib.clone()));
        assert!(!library.set_item_background(1, salib.clone()));
        assert!(library.set_item_background(2, salib.clone()));
        assert!(!library.set_item_background(99, salib.clone()));
        // Materi sementara selalu memakai latar umum.
        let scratch = library.set_scratch(Kind::Verse, "A".into(), "A".into(), None);
        assert!(!library.set_item_background(scratch, salib.clone()));
        assert_eq!(library.item(scratch).unwrap().background, None);
        assert_eq!(library.background_users("Salib.jpg"), 2);
        assert_eq!(library.forget_item_background("Salib.jpg"), 2);
        assert_eq!(library.background_users("Salib.jpg"), 0);
        assert!(library.items.iter().all(|i| i.background.is_none()));
        // library.json lama tidak punya kolom ini.
        let old: Item =
            serde_json::from_str(r#"{"id":1,"kind":"Song","title":"A","content":"B"}"#).unwrap();
        assert_eq!(old.background, None);
    }
}
