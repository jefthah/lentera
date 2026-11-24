//! Akun yang sedang masuk di satu folder data (`akun.json`).
//!
//! Token sesi tidak disimpan di sini, tetapi di penyimpanan rahasia sistem
//! operasi (Windows Credential Manager / macOS Keychain) oleh aplikasi.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

/// Nama berkas akun di folder data.
pub const ACCOUNT_FILE: &str = "akun.json";

/// Akun yang masuk di folder data ini.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedIn {
    /// Alamat server, misalnya `https://api.contoh.org`.
    pub server: String,
    /// Id akun di server.
    pub account: String,
    pub email: String,
    /// Profil yang dihapus di perangkat ini. Profil ini tidak diunduh lagi
    /// dari server, dan dihapus di server bila akun ini pemiliknya.
    #[serde(default)]
    pub removed: Vec<String>,
}

impl SignedIn {
    /// Membaca `akun.json`; `None` bila belum ada (belum pernah masuk).
    pub fn load(data_dir: &Path) -> Result<Option<Self>, String> {
        let path = data_dir.join(ACCOUNT_FILE);
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| format!("Berkas akun {} rusak.\n{e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!(
                "Berkas akun {} tidak terbaca.\n{e}",
                path.display()
            )),
        }
    }

    /// Menyimpan `akun.json` lewat berkas sementara.
    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        let path = data_dir.join(ACCOUNT_FILE);
        let temp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&temp, text)
            .and_then(|()| fs::rename(&temp, &path))
            .map_err(|e| format!("Berkas akun {} tidak bisa disimpan.\n{e}", path.display()))
    }

    /// Menghapus `akun.json` saat keluar. Data profil tidak disentuh.
    pub fn forget(data_dir: &Path) -> Result<(), String> {
        match fs::remove_file(data_dir.join(ACCOUNT_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("Berkas akun tidak bisa dihapus.\n{e}"))
            }
            _ => Ok(()),
        }
    }

    /// Mencatat profil yang dihapus di perangkat ini.
    pub fn remember_removed(&mut self, id: &str) {
        if !self.removed.iter().any(|r| r == id) {
            self.removed.push(id.into());
        }
    }
}
