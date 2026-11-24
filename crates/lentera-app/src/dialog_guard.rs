//! Penjaga kotak pesan Windows yang tidak mau tampil.
//!
//! Windows baru menampilkan kotak pesan (`MessageBox`) saat antrean pesan
//! thread UI kosong. Bila kotak pesan dibuka di tengah callback Slint ketika
//! jendela masih perlu digambar ulang, misalnya tepat setelah menu sembulan
//! seperti menu akun ditutup, winit terus mengulang permintaan gambar ulang.
//! Antrean tidak pernah kosong, kotak pesan tetap tersembunyi, dan jendela
//! Lentera terkunci menunggu jawabannya, sehingga Lentera tampak macet.
//!
//! Selama kotak pesan terbuka, [`DialogGuard`] memeriksa dari thread lain
//! apakah kotak pesan berjudul sama masih tersembunyi, lalu memunculkannya.
//! Hanya fungsi yang tidak mengirim pesan ke thread UI yang dipakai, supaya
//! penjaga tidak pernah menunggu thread UI.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, Thread};
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetClassNameW, InternalGetWindowText, IsWindowVisible, SW_SHOWNORMAL,
    ShowWindowAsync,
};

/// Kelas jendela kotak dialog Windows.
const DIALOG_CLASS: &str = "#32770";
/// Waktu tunggu sebelum kotak pesan yang belum tampil dimunculkan. Dalam
/// keadaan biasa Windows sudah menampilkannya jauh sebelum ini.
const WAIT: Duration = Duration::from_millis(250);

/// Aktif sejak dibuat sampai di-drop. Buat tepat sebelum kotak pesan dibuka,
/// di thread UI.
pub struct DialogGuard {
    stop: Arc<AtomicBool>,
    thread: Thread,
}

impl DialogGuard {
    pub fn start(title: &str) -> Self {
        // SAFETY: tanpa prasyarat; mengembalikan id thread pemanggil.
        let ui_thread = unsafe { GetCurrentThreadId() };
        let title: Vec<u16> = title.encode_utf16().collect();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = thread::spawn(move || {
            loop {
                thread::park_timeout(WAIT);
                if flag.load(Ordering::Acquire) {
                    break;
                }
                show_hidden(ui_thread, &title);
            }
        });
        Self {
            stop,
            thread: handle.thread().clone(),
        }
    }
}

impl Drop for DialogGuard {
    fn drop(&mut self) {
        // Tidak menunggu thread penjaga selesai: kotak pesan sudah ditutup,
        // dan thread itu berhenti sendiri pada pemeriksaan berikutnya.
        self.stop.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

/// Memunculkan kotak dialog berjudul `title` milik `ui_thread` yang masih
/// tersembunyi.
fn show_hidden(ui_thread: u32, title: &[u16]) {
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `data` menunjuk `title` di `show_hidden`, yang masih hidup
        // selama `EnumThreadWindows` berjalan.
        let title = unsafe { &*(data as *const &[u16]) };
        let mut buffer = [0u16; 256];
        // SAFETY: `hwnd` diberikan EnumThreadWindows dan panjang buffer benar.
        // Ketiga fungsi ini tidak mengirim pesan ke thread pemilik jendela.
        unsafe {
            let len = GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if !same(&buffer, len, DIALOG_CLASS) || IsWindowVisible(hwnd) != 0 {
                return 1;
            }
            let len = InternalGetWindowText(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if usize::try_from(len).ok().and_then(|len| buffer.get(..len)) == Some(*title) {
                ShowWindowAsync(hwnd, SW_SHOWNORMAL);
            }
        }
        1
    }
    // SAFETY: `visit` hanya membaca `title` lewat penunjuk yang sah selama
    // pemanggilan ini.
    unsafe {
        EnumThreadWindows(ui_thread, Some(visit), &title as *const &[u16] as LPARAM);
    }
}

fn same(buffer: &[u16], len: i32, text: &str) -> bool {
    usize::try_from(len)
        .ok()
        .and_then(|len| buffer.get(..len))
        .is_some_and(|got| got.iter().copied().eq(text.encode_utf16()))
}
