//! Pemeriksaan khusus Windows untuk uji otomatis: posisi jendela menurut
//! Win32, tangkapan isi jendela output sendiri, serta RAM dan CPU proses.

use std::ffi::c_void;

use windows::Win32::Foundation::{FILETIME, HWND, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PW_CLIENTONLY, PrintWindow};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, PW_RENDERFULLCONTENT};

pub fn hwnd_from(raw: raw_window_handle::RawWindowHandle) -> Option<HWND> {
    match raw {
        raw_window_handle::RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut c_void)),
        _ => None,
    }
}

pub struct Placement {
    pub window: [i32; 4],
    pub monitor: [i32; 4],
}

/// Persegi jendela dan persegi monitor tempat jendela itu berada, menurut Win32.
pub fn placement(hwnd: HWND) -> Option<Placement> {
    unsafe {
        let mut wr = RECT::default();
        GetWindowRect(hwnd, &mut wr).ok()?;
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return None;
        }
        let r = |r: RECT| [r.left, r.top, r.right, r.bottom];
        Some(Placement { window: r(wr), monitor: r(mi.rcMonitor) })
    }
}

/// Tangkap isi area klien jendela kita sendiri sebagai RGBA.
pub fn capture(hwnd: HWND) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let mut cr = RECT::default();
        GetClientRect(hwnd, &mut cr).ok()?;
        let (w, h) = (cr.right - cr.left, cr.bottom - cr.top);
        if w <= 0 || h <= 0 {
            return None;
        }
        let hdc = GetDC(Some(hwnd));
        let mem = CreateCompatibleDC(Some(hdc));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let old = SelectObject(mem, dib.into());
        let ok = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(PW_CLIENTONLY.0 | PW_RENDERFULLCONTENT)).as_bool();
        let mut rgba = Vec::new();
        if ok && !bits.is_null() {
            let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
            rgba.reserve(src.len());
            for px in src.chunks_exact(4) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        SelectObject(mem, old);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem);
        ReleaseDC(Some(hwnd), hdc);
        ok.then_some((w as u32, h as u32, rgba))
    }
}

pub struct Memory {
    pub working_set: u64,
    pub peak_working_set: u64,
    pub private: u64,
}

pub fn memory() -> Option<Memory> {
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            c.cb,
        )
        .ok()?;
        Some(Memory {
            working_set: c.WorkingSetSize as u64,
            peak_working_set: c.PeakWorkingSetSize as u64,
            private: c.PrivateUsage as u64,
        })
    }
}

/// Total waktu CPU (kernel + user) proses ini, dalam detik.
pub fn cpu_seconds() -> Option<f64> {
    unsafe {
        let (mut c, mut e, mut k, mut u) =
            (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).ok()?;
        let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64 / 1e7;
        Some(t(k) + t(u))
    }
}
