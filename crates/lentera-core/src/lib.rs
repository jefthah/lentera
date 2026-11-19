//! Inti Lentera: pustaka materi, susunan ibadah, impor, dan penyimpanan.
//!
//! Crate ini tidak memuat kode tampilan dan tidak memanggil API khusus
//! Windows atau macOS, sehingga dapat dipakai tampilan Win32 sekarang dan
//! tampilan Slint nanti.

mod import;
mod library;
mod service;
mod storage;
mod store;

pub use import::{MAX_IMPORT_BYTES, normalize_text, read_text_file};
pub use library::{Item, Kind, Library, split_slides};
pub use service::{Cursor, Slide, Step};
pub use storage::{DataLock, LIBRARY_FILE, LOCK_FILE, default_data_dir};
pub use store::{DEFAULT_PROFILE_NAME, Opened, PROFILE_DB, PROFILES_DIR, Profile, Store, profiles};
