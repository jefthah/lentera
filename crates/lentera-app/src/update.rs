//! Pembaruan otomatis lewat Velopack.
//!
//! Saat Lentera dibuka, thread latar memeriksa GitHub Releases lalu mengunduh
//! versi baru bila ada. Setelah siap, `ready` dipanggil dengan nomor versinya.
//! Pembaruan baru dipasang setelah Lentera ditutup, atau segera bila operator
//! menekan tombol "Perbarui". Tidak ada yang dipasang selama Lentera berjalan,
//! jadi tayangan tidak pernah terputus oleh pembaruan.

use std::{
    ffi::OsString,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use velopack::{
    UpdateCheck, UpdateManager, VelopackApp, VelopackAsset,
    sources::{AutoSource, GithubSource},
};

/// Repositori rilis. Pre-release ikut diperiksa selama Lentera masih beta.
const REPOSITORY: &str = "https://github.com/jefthah/lentera";

struct Pending {
    manager: UpdateManager,
    asset: VelopackAsset,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static RESTART: AtomicBool = AtomicBool::new(false);

/// Wajib dipanggil paling awal di `main`. Menangani perintah dari installer
/// (pasang, hapus, pembaruan); untuk perintah itu proses langsung keluar.
pub fn startup() {
    VelopackApp::build().run();
}

/// Memeriksa dan mengunduh pembaruan di thread latar. Diam saja bila Lentera
/// tidak dipasang lewat installer (misalnya hasil `cargo build`), bila tidak
/// ada internet, atau bila sudah versi terbaru.
///
/// `source` mengganti sumber pembaruan (folder lokal atau URL); dipakai untuk
/// pengujian.
pub fn check_in_background(source: Option<String>, ready: impl FnOnce(String) + Send + 'static) {
    std::thread::spawn(move || {
        let manager = match &source {
            Some(source) => UpdateManager::new(AutoSource::new(source), None, None),
            None => UpdateManager::new(GithubSource::new(REPOSITORY, None, true), None, None),
        };
        let Ok(manager) = manager else {
            return;
        };
        let asset = match manager.get_update_pending_restart() {
            Some(asset) => asset,
            None => match manager.check_for_updates() {
                Ok(UpdateCheck::UpdateAvailable(info)) => {
                    if manager.download_updates(&info, None).is_err() {
                        return;
                    }
                    info.TargetFullRelease.clone()
                }
                _ => return,
            },
        };
        let version = asset.Version.clone();
        if let Ok(mut pending) = PENDING.lock() {
            *pending = Some(Pending { manager, asset });
        }
        ready(version);
    });
}

/// Buka Lentera lagi setelah pembaruan dipasang.
pub fn request_restart() {
    RESTART.store(true, Ordering::SeqCst);
}

/// Dipanggil tepat sebelum proses keluar. Velopack menunggu Lentera keluar,
/// memasang pembaruan, lalu membuka Lentera lagi bila [`request_restart`]
/// dipanggil. Tanpa restart, pemasangan berjalan tanpa jendela.
///
/// `args` dipakai saat Lentera dibuka lagi, supaya `--data-dir` tetap sama.
pub fn apply_after_exit(args: &[OsString]) {
    let pending = PENDING.lock().ok().and_then(|mut p| p.take());
    if let Some(pending) = pending {
        let restart = RESTART.load(Ordering::SeqCst);
        // Bila gagal, Velopack memasang paket yang sudah diunduh saat
        // Lentera dibuka berikutnya.
        let _ = pending.manager.wait_exit_then_apply_updates(
            &pending.asset,
            !restart,
            restart,
            args.iter(),
        );
    }
}
