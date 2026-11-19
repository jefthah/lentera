use std::{
    ffi::OsString,
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
};

/// Berkas pustaka Lentera ≤ 0.2.x. Sejak profil, isinya dipindahkan sekali ke
/// profil pertama (lihat [`Store::open`](crate::Store::open)).
pub const LIBRARY_FILE: &str = "library.json";

/// Berkas kunci di folder data. Namanya sama dengan versi lama, supaya Lentera
/// lama dan baru tidak bisa membuka folder yang sama bersamaan.
pub const LOCK_FILE: &str = "library.lock";

/// Folder data bawaan untuk sistem operasi yang sedang berjalan:
///
/// - Windows: `%LOCALAPPDATA%\Lentera` (sama dengan Lentera 0.1.0)
/// - macOS: `~/Library/Application Support/Lentera`
/// - lainnya: `$XDG_DATA_HOME/lentera`, atau `~/.local/share/lentera`
pub fn default_data_dir() -> PathBuf {
    data_dir_for(std::env::consts::OS, |name| std::env::var_os(name))
}

fn data_dir_for(os: &str, var: impl Fn(&str) -> Option<OsString>) -> PathBuf {
    let dir = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let here = || PathBuf::from(".");
    match os {
        "windows" => dir("LOCALAPPDATA").unwrap_or_else(here).join("Lentera"),
        "macos" => dir("HOME")
            .unwrap_or_else(here)
            .join("Library/Application Support/Lentera"),
        _ => dir("XDG_DATA_HOME")
            .filter(|path| path.has_root())
            .or_else(|| dir("HOME").map(|home| home.join(".local/share")))
            .unwrap_or_else(here)
            .join("lentera"),
    }
}

/// Kunci agar hanya satu proses Lentera memakai satu folder data.
/// Kunci lepas saat nilai ini di-drop atau proses berhenti, termasuk saat crash.
#[derive(Debug)]
pub struct DataLock {
    _file: File,
}

impl DataLock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| {
                format!("Pustaka sedang dibuka jendela lain atau tidak dapat ditulis.\n{e}")
            })?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(
                "Pustaka sedang dibuka jendela Lentera lain. Tutup jendela itu terlebih dahulu."
                    .into(),
            ),
            Err(TryLockError::Error(e)) => Err(format!("Pustaka tidak dapat dikunci.\n{e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn data_folder_follows_each_platform_convention() {
        let local = r"C:\Users\a\AppData\Local";
        assert_eq!(
            data_dir_for("windows", env(&[("LOCALAPPDATA", local)])),
            PathBuf::from(local).join("Lentera")
        );
        assert_eq!(
            data_dir_for("macos", env(&[("HOME", "/Users/a")])),
            PathBuf::from("/Users/a").join("Library/Application Support/Lentera")
        );
        assert_eq!(
            data_dir_for(
                "linux",
                env(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "/data")])
            ),
            PathBuf::from("/data").join("lentera")
        );
        // XDG_DATA_HOME relatif harus diabaikan menurut spesifikasi XDG.
        assert_eq!(
            data_dir_for(
                "linux",
                env(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "rel")])
            ),
            PathBuf::from("/home/a")
                .join(".local/share")
                .join("lentera")
        );
        assert_eq!(
            data_dir_for("windows", env(&[("LOCALAPPDATA", "")])),
            PathBuf::from(".").join("Lentera")
        );
    }

    #[test]
    fn second_lock_on_same_folder_fails_until_first_is_released() {
        let dir = std::env::temp_dir().join(format!("lentera-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("library.lock");
        let first = DataLock::acquire(&path).unwrap();
        assert!(
            DataLock::acquire(&path)
                .unwrap_err()
                .contains("jendela Lentera lain")
        );
        drop(first);
        drop(DataLock::acquire(&path).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
