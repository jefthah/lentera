use crate::{Kind, Library};
use std::collections::HashSet;
use std::{fs, path::Path};

/// Rencana impor banyak lagu sekaligus, misalnya dari EasyWorship. Dibuat
/// dengan [`Library::plan_song_import`] supaya jumlahnya bisa ditunjukkan
/// dulu, lalu dijalankan dengan [`Library::add_songs`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SongImport {
    /// Judul (sudah diberi nomor bila perlu) dan lirik lagu yang ditambahkan.
    pub songs: Vec<(String, String)>,
    /// Lagu yang dilewati karena judul dan liriknya sudah ada di pustaka, atau
    /// muncul dua kali di berkas.
    pub existing: usize,
    /// Lagu baru yang judulnya diberi nomor, misalnya "Kasih (2)", karena
    /// judul itu sudah dipakai lagu lain dengan lirik berbeda.
    pub renamed: usize,
}

/// Huruf dan angka saja, huruf kecil: "Kasih-Mu, Tuhan!" menjadi
/// "kasihmutuhan". Dipakai untuk mengenali lagu yang sama.
fn fold(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Judul tanpa nomor pembeda di belakang: "Kasih (2)" menjadi "Kasih".
fn base_title(title: &str) -> &str {
    let title = title.trim_end();
    if let Some(inner) = title.strip_suffix(')')
        && let Some(open) = inner.rfind(" (")
        && inner.len() > open + 2
        && inner[open + 2..].bytes().all(|b| b.is_ascii_digit())
    {
        return inner[..open].trim_end();
    }
    title
}

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

    /// Menyiapkan impor banyak lagu tanpa mengubah pustaka. Lagu yang judul
    /// dan liriknya sudah ada dilewati (tanda baca, spasi, huruf besar/kecil,
    /// dan nomor seperti "(2)" di belakang judul diabaikan), jadi impor yang
    /// sama boleh diulang. Lagu dengan judul yang sudah dipakai tetapi lirik
    /// berbeda diberi nomor: "Kasih (2)".
    pub fn plan_song_import(
        &self,
        songs: impl IntoIterator<Item = (String, String)>,
    ) -> SongImport {
        let library_songs = self.items.iter().filter(|i| i.kind == Kind::Song);
        let mut known: HashSet<(String, String)> = library_songs
            .clone()
            .map(|i| (fold(base_title(&i.title)), fold(&i.content)))
            .collect();
        let mut titles: HashSet<String> = library_songs
            .map(|i| i.title.trim().to_lowercase())
            .collect();
        let mut plan = SongImport::default();
        for (title, content) in songs {
            if !known.insert((fold(base_title(&title)), fold(&content))) {
                plan.existing += 1;
                continue;
            }
            let mut unique = title.trim().to_owned();
            if titles.contains(&unique.to_lowercase()) {
                let base = base_title(&unique).to_owned();
                unique = (2..)
                    .map(|n| format!("{base} ({n})"))
                    .find(|t| !titles.contains(&t.to_lowercase()))
                    .unwrap_or(base);
                plan.renamed += 1;
            }
            titles.insert(unique.to_lowercase());
            plan.songs.push((unique, content));
        }
        plan
    }

    /// Menambahkan lagu dari [`Library::plan_song_import`]. Hasilnya id lagu
    /// yang ditambahkan.
    pub fn add_songs(&mut self, plan: SongImport) -> Vec<u64> {
        plan.songs
            .into_iter()
            .map(|(title, content)| self.add(Kind::Song, title, content))
            .collect()
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

    fn songs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(t, c)| (t.to_string(), c.to_string()))
            .collect()
    }

    #[test]
    fn song_import_skips_known_songs_and_numbers_title_variants() {
        let mut library = Library::blank();
        library.add(Kind::Song, "Kasih".into(), "Kasih-Mu, Tuhan\n\nAmin".into());
        library.add(Kind::Verse, "Mazmur 23".into(), "Tuhan gembalaku".into());
        let incoming = songs(&[
            // Sama dengan lagu di pustaka; tanda baca dan huruf besar diabaikan.
            ("KASIH", "kasih mu tuhan\namin"),
            ("Kasih", "Lirik lain"),
            ("Kasih", "Lirik ketiga"),
            ("Mazmur 23", "Lagu, bukan ayat"),
            ("Baru", "Lagu baru"),
            ("Baru", "Lagu baru"),
            ("baru", "Versi kedua"),
        ]);
        let plan = library.plan_song_import(incoming.clone());
        assert_eq!(
            plan.songs,
            songs(&[
                ("Kasih (2)", "Lirik lain"),
                ("Kasih (3)", "Lirik ketiga"),
                ("Mazmur 23", "Lagu, bukan ayat"),
                ("Baru", "Lagu baru"),
                ("baru (2)", "Versi kedua"),
            ])
        );
        assert_eq!((plan.existing, plan.renamed), (2, 3));
        // Rencana tidak mengubah pustaka.
        assert_eq!(library.items.len(), 2);

        let ids = library.add_songs(plan);
        assert_eq!(ids.len(), 5);
        assert_eq!(library.item(ids[0]).unwrap().title, "Kasih (2)");
        assert!(
            ids.iter()
                .all(|id| library.item(*id).unwrap().kind == Kind::Song)
        );

        // Impor yang sama diulang: tidak ada yang ditambahkan.
        let again = library.plan_song_import(incoming);
        assert!(again.songs.is_empty());
        assert_eq!(again.existing, 7);
    }

    #[test]
    fn base_titles() {
        assert_eq!(base_title("Kasih (2)"), "Kasih");
        assert_eq!(base_title("Kasih (12) "), "Kasih");
        assert_eq!(base_title("Mazmur (Daud)"), "Mazmur (Daud)");
        assert_eq!(base_title("Lagu ()"), "Lagu ()");
        assert_eq!(base_title("(2)"), "(2)");
    }
}
