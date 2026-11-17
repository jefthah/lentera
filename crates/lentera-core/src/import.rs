use crate::{Kind, Library};
use std::{fs, path::Path};

/// Batas ukuran berkas yang boleh diimpor.
pub const MAX_IMPORT_BYTES: u64 = 32 * 1024 * 1024;

/// Membaca berkas teks UTF-8 untuk diimpor. Berkas di atas
/// [`MAX_IMPORT_BYTES`] ditolak sebelum dibaca.
pub fn read_text_file(path: &Path) -> Result<String, String> {
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_IMPORT_BYTES {
        return Err("Berkas terlalu besar. Batas impor 32 MB.".into());
    }
    fs::read_to_string(path).map_err(|e| format!("Gunakan berkas teks UTF-8.\n{e}"))
}

/// Membuang BOM UTF-8 di awal teks dan mengubah akhir baris CRLF menjadi LF.
pub fn normalize_text(text: &str) -> String {
    text.trim_start_matches('\u{feff}').replace("\r\n", "\n")
}

impl Library {
    /// Mengimpor satu lagu dari berkas TXT. Judul diambil dari nama berkas.
    pub fn import_song_file(&mut self, path: &Path) -> Result<u64, String> {
        let input = read_text_file(path)?;
        let title = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        Ok(self.add(Kind::Song, title, normalize_text(&input)))
    }

    /// Mengimpor ayat dari berkas TSV; lihat [`Library::import_verses`].
    pub fn import_verses_file(&mut self, path: &Path) -> Result<usize, String> {
        self.import_verses(&read_text_file(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lentera-import-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn song_file_takes_title_from_file_name_and_normalizes_text() {
        let dir = temp_dir("song");
        let path = dir.join("Kasih Setia.txt");
        fs::write(&path, "\u{feff}Baris 1\r\nBaris 2\r\n\r\nReff").unwrap();
        let mut library = Library::default();
        let id = library.import_song_file(&path).unwrap();
        let item = library.item(id).unwrap();
        assert_eq!(item.kind, Kind::Song);
        assert_eq!(item.title, "Kasih Setia");
        assert_eq!(item.content, "Baris 1\nBaris 2\n\nReff");
        assert_eq!(item.slides().len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn large_missing_and_non_utf8_files_change_nothing() {
        let dir = temp_dir("reject");
        let large = dir.join("besar.txt");
        fs::File::create(&large)
            .unwrap()
            .set_len(MAX_IMPORT_BYTES + 1)
            .unwrap();
        let binary = dir.join("bukan-utf8.tsv");
        fs::write(&binary, [0xff, 0xfe, 0x00, b'\t']).unwrap();
        let mut library = Library::default();
        let before = library.items.len();
        assert!(
            library
                .import_song_file(&large)
                .unwrap_err()
                .contains("32 MB")
        );
        assert!(
            library
                .import_verses_file(&binary)
                .unwrap_err()
                .contains("UTF-8")
        );
        assert!(
            library
                .import_song_file(&dir.join("tidak-ada.txt"))
                .is_err()
        );
        assert_eq!(library.items.len(), before);
        fs::remove_dir_all(dir).unwrap();
    }
}
