//! Inti Lentera: pustaka materi, susunan ibadah, impor, dan penyimpanan.
//!
//! Crate ini tidak memuat kode tampilan dan tidak memanggil API khusus
//! Windows atau macOS, sehingga dapat dipakai tampilan Win32 sekarang dan
//! tampilan Slint nanti.

pub mod background;
pub mod bible;
pub mod easyworship;
mod import;
mod library;
mod service;
mod storage;
mod store;
pub mod sync;

pub use import::{MAX_IMPORT_BYTES, SongImport, normalize_text, read_text_file};
pub use library::{
    Item, Kind, Library, MAIN_SERVICE_NAME, SCRATCH_ID_START, SERVICE_NAME_MAX, SERVICES_MAX,
    SavedService, clean_text, split_slides,
};
pub use service::{Cursor, Slide, Step};
pub use storage::{DataLock, LIBRARY_FILE, LOCK_FILE, default_data_dir};
pub use store::{
    DEFAULT_PROFILE_NAME, DELETED_PROFILES_DIR, Opened, PROFILE_DB, PROFILE_NAME_MAX, PROFILES_DIR,
    Profile, Store, profiles, remove_profile, rename_profile,
};
