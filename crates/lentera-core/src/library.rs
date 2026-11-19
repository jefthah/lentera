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
}

impl Item {
    pub fn slides(&self) -> Vec<String> {
        split_slides(&self.content)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub version: u32,
    pub next_id: u64,
    pub items: Vec<Item>,
    pub service: Vec<u64>,
    pub background: usize,
    pub monitor: usize,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1, next_id: 3,
            items: vec![
                Item { id: 1, kind: Kind::Song, title: "Selamat datang di Lentera".into(),
                    content: "Siapkan hati untuk beribadah\nBersama memuji dan bersyukur\n\nKasih Tuhan menyertai kita\nHari ini dan selamanya".into() },
                Item { id: 2, kind: Kind::Verse, title: "Ayat contoh — ganti dengan referensi".into(),
                    content: "Masukkan teks ayat dari terjemahan\nyang Anda berhak gunakan.\n\nAnda juga bisa mengimpor ayat\ndari berkas TSV UTF-8.".into() },
            ],
            service: vec![1, 2], background: 0, monitor: 0,
        }
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

impl Library {
    pub fn item(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
    pub fn add(&mut self, kind: Kind, title: String, content: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Item {
            id,
            kind,
            title,
            content,
        });
        id
    }
    pub fn remove(&mut self, id: u64) {
        self.items.retain(|i| i.id != id);
        self.service.retain(|i| *i != id);
    }
    /// Menyimpan isi editor ke materi. Judul kosong menjadi "Tanpa judul".
    pub fn update(&mut self, id: u64, title: &str, content: &str) -> bool {
        let Some(item) = self.items.iter_mut().find(|i| i.id == id) else {
            return false;
        };
        let title = title.trim();
        item.title = if title.is_empty() {
            "Tanpa judul".into()
        } else {
            title.into()
        };
        item.content = content.replace("\r\n", "\n");
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
            .take(500)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slides_keep_unicode_and_ignore_repeated_blank_lines() {
        assert_eq!(
            split_slides("  \r\nKasih-Mu\r\nTuhan 🙏\r\n \r\n\r\nAmin\r\n"),
            vec!["Kasih-Mu\nTuhan 🙏", "Amin"]
        );
        assert!(split_slides(" \n\n").is_empty());
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
    fn invalid_ids_and_future_formats_are_rejected() {
        let mut library = Library::default();
        library.items[1].id = 1;
        assert!(library.validate().is_err());
        library = Library::default();
        library.version = 99;
        assert!(library.validate().is_err());
    }
}
