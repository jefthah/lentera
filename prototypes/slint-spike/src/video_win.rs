//! Pemutar video memakai Media Foundation (bawaan Windows).
//!
//! Decoder berjalan di thread sendiri. Setiap frame diubah ke RGBA lalu
//! dikirim ke thread UI. Kalau UI belum sempat menampilkan frame sebelumnya,
//! frame baru dibuang supaya antrean tidak membengkak di PC lemah.
//!
//! Dua mode decode:
//! - `hw = true`: decode dan konversi warna di GPU (DXVA lewat Direct3D 11),
//!   lalu frame disalin ke memori biasa. Ini jalur yang dituju untuk PC lemah.
//! - `hw = false`: decode dan konversi warna di CPU.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use slint::{Rgba8Pixel, SharedPixelBuffer};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_9_3, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::{HSTRING, Interface};

#[derive(Default)]
pub struct VideoStats {
    pub decoded: AtomicU64,
    pub dropped_busy: AtomicU64,
    pub late: AtomicU64,
    pub convert_ns: AtomicU64,
    pub width: AtomicU64,
    pub height: AtomicU64,
    pub loops: AtomicU64,
    /// Benar kalau frame yang diterima berasal dari permukaan GPU (DXVA).
    pub gpu_surface: AtomicBool,
    /// Benar kalau perangkat Direct3D 11 untuk decode GPU berhasil dibuat.
    pub hw_ready: AtomicBool,
}

pub struct VideoHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl VideoHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// `deliver` dipanggil dari thread decoder. Ia mengembalikan `false` kalau
/// frame dibuang karena UI masih sibuk.
pub fn start<F>(path: PathBuf, hw: bool, stats: Arc<VideoStats>, deliver: F) -> VideoHandle
where
    F: Fn(SharedPixelBuffer<Rgba8Pixel>) -> bool + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let join = std::thread::Builder::new()
        .name("lentera-video".into())
        .spawn(move || {
            if let Err(e) = run(&path, hw, &stop2, &stats, &deliver) {
                eprintln!("video: {e}");
            }
        })
        .expect("thread video");
    VideoHandle { stop, join: Some(join) }
}

const FIRST_VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

fn run<F>(path: &PathBuf, hw: bool, stop: &AtomicBool, stats: &VideoStats, deliver: &F) -> windows::core::Result<()>
where
    F: Fn(SharedPixelBuffer<Rgba8Pixel>) -> bool,
{
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET)?;
    }
    let result = (|| {
        // Perangkat GPU dibuat sekali dan dipakai ulang setiap video berulang.
        let gpu = if hw {
            match Gpu::new() {
                Ok(g) => {
                    stats.hw_ready.store(true, Ordering::Relaxed);
                    Some(g)
                }
                Err(e) => {
                    eprintln!("video: decode GPU tidak tersedia, pakai CPU ({e})");
                    None
                }
            }
        } else {
            None
        };
        while !stop.load(Ordering::SeqCst) {
            play_once(path, gpu.as_ref(), stop, stats, deliver)?;
            stats.loops.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    })();
    unsafe {
        let _ = MFShutdown();
        CoUninitialize();
    }
    result
}

struct Gpu {
    _device: ID3D11Device,
    manager: IMFDXGIDeviceManager,
}

impl Gpu {
    fn new() -> windows::core::Result<Self> {
        unsafe {
            let levels = [
                D3D_FEATURE_LEVEL_11_1,
                D3D_FEATURE_LEVEL_11_0,
                D3D_FEATURE_LEVEL_10_1,
                D3D_FEATURE_LEVEL_10_0,
                D3D_FEATURE_LEVEL_9_3,
            ];
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device = device.expect("perangkat D3D11");
            // Media Foundation memakai perangkat ini dari beberapa thread.
            if let Some(ctx) = context {
                if let Ok(mt) = ctx.cast::<ID3D11Multithread>() {
                    let _ = mt.SetMultithreadProtected(true);
                }
            }
            let mut token = 0u32;
            let mut manager: Option<IMFDXGIDeviceManager> = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
            let manager = manager.expect("DXGI manager");
            manager.ResetDevice(&device, token)?;
            Ok(Gpu { _device: device, manager })
        }
    }
}

/// Ukuran frame dari decoder, plus bagian yang benar-benar ditampilkan.
/// Video 1080p H.264 biasanya didecode 1088 baris; 8 baris terakhir harus dibuang.
struct Format {
    height: u32,
    stride: i32,
    crop_x: u32,
    crop_y: u32,
    show_w: u32,
    show_h: u32,
}

fn open_reader(path: &PathBuf, gpu: Option<&Gpu>) -> windows::core::Result<IMFSourceReader> {
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 3)?;
        let attrs = attrs.expect("atribut MF");
        match gpu {
            Some(g) => {
                attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &g.manager)?;
                attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
                attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            }
            // Biarkan Media Foundation mengubah YUV ke RGB32 di CPU.
            None => attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?,
        }
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attrs)?;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        reader.SetStreamSelection(FIRST_VIDEO, true)?;
        let mt = MFCreateMediaType()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        reader.SetCurrentMediaType(FIRST_VIDEO, None, &mt)?;
        Ok(reader)
    }
}

fn current_format(reader: &IMFSourceReader) -> windows::core::Result<Format> {
    unsafe {
        let mt = reader.GetCurrentMediaType(FIRST_VIDEO)?;
        let size = mt.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let width = (size >> 32) as u32;
        let height = (size & 0xffff_ffff) as u32;
        let stride = mt
            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
            .map(|s| s as i32)
            .unwrap_or((width * 4) as i32);
        let mut area = MFVideoArea::default();
        let bytes = std::slice::from_raw_parts_mut(
            &mut area as *mut MFVideoArea as *mut u8,
            size_of::<MFVideoArea>(),
        );
        let (crop_x, crop_y, show_w, show_h) = match mt.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, bytes, None) {
            Ok(()) if area.Area.cx > 0 && area.Area.cy > 0 => {
                let x = area.OffsetX.value.max(0) as u32;
                let y = area.OffsetY.value.max(0) as u32;
                (x, y, (area.Area.cx as u32).min(width - x), (area.Area.cy as u32).min(height - y))
            }
            _ => (0, 0, width, height),
        };
        Ok(Format { height, stride, crop_x, crop_y, show_w, show_h })
    }
}

fn play_once<F>(path: &PathBuf, gpu: Option<&Gpu>, stop: &AtomicBool, stats: &VideoStats, deliver: &F) -> windows::core::Result<()>
where
    F: Fn(SharedPixelBuffer<Rgba8Pixel>) -> bool,
{
    let reader = open_reader(path, gpu)?;
    let mut fmt = current_format(&reader)?;
    stats.width.store(fmt.show_w as u64, Ordering::Relaxed);
    stats.height.store(fmt.show_h as u64, Ordering::Relaxed);

    let start = Instant::now();
    let mut first_ts: Option<i64> = None;

    while !stop.load(Ordering::SeqCst) {
        let mut flags = 0u32;
        let mut ts = 0i64;
        let mut sample: Option<IMFSample> = None;
        unsafe {
            reader.ReadSample(FIRST_VIDEO, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample))?;
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
        if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
            fmt = current_format(&reader)?;
            stats.width.store(fmt.show_w as u64, Ordering::Relaxed);
            stats.height.store(fmt.show_h as u64, Ordering::Relaxed);
        }
        let Some(sample) = sample else { continue };

        let t0 = Instant::now();
        let frame = unsafe { to_rgba(&sample, &fmt, stats)? };
        stats.convert_ns.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
        stats.decoded.fetch_add(1, Ordering::Relaxed);

        // Tunggu sampai waktu tayang frame ini (timestamp dalam satuan 100 ns).
        let base = *first_ts.get_or_insert(ts);
        let due = start + Duration::from_nanos(((ts - base).max(0) as u64) * 100);
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        } else if now - due > Duration::from_millis(50) {
            stats.late.fetch_add(1, Ordering::Relaxed);
        }

        if !deliver(frame) {
            stats.dropped_busy.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(())
}

unsafe fn to_rgba(sample: &IMFSample, fmt: &Format, stats: &VideoStats) -> windows::core::Result<SharedPixelBuffer<Rgba8Pixel>> {
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(fmt.show_w, fmt.show_h);
    let dst = out.make_mut_bytes();

    let buffer = unsafe { sample.GetBufferByIndex(0)? };
    if buffer.cast::<IMFDXGIBuffer>().is_ok() {
        stats.gpu_surface.store(true, Ordering::Relaxed);
    }
    // Utamakan IMF2DBuffer karena memberi pitch yang benar (bisa negatif).
    // Untuk permukaan GPU, Lock2D menyalin isinya ke memori biasa.
    if let Ok(b2d) = buffer.cast::<IMF2DBuffer>() {
        let mut scan0: *mut u8 = std::ptr::null_mut();
        let mut pitch = 0i32;
        unsafe { b2d.Lock2D(&mut scan0, &mut pitch)? };
        unsafe { copy_rows(scan0, pitch as isize, fmt, dst) };
        unsafe { b2d.Unlock2D()? };
    } else {
        let buffer = unsafe { sample.ConvertToContiguousBuffer()? };
        let mut ptr: *mut u8 = std::ptr::null_mut();
        unsafe { buffer.Lock(&mut ptr, None, None)? };
        let stride = fmt.stride as isize;
        let scan0 = if stride < 0 {
            unsafe { ptr.offset(-stride * (fmt.height as isize - 1)) }
        } else {
            ptr
        };
        unsafe { copy_rows(scan0, stride, fmt, dst) };
        unsafe { buffer.Unlock()? };
    }
    Ok(out)
}

/// Salin bagian yang ditampilkan dari baris BGRX ke RGBA.
unsafe fn copy_rows(scan0: *const u8, pitch: isize, fmt: &Format, dst: &mut [u8]) {
    let (w, h) = (fmt.show_w as usize, fmt.show_h as usize);
    let x0 = fmt.crop_x as usize * 4;
    for y in 0..h {
        let row_start = unsafe { scan0.offset(pitch * (y + fmt.crop_y as usize) as isize).add(x0) };
        let src = unsafe { std::slice::from_raw_parts(row_start, w * 4) };
        let row = &mut dst[y * w * 4..(y + 1) * w * 4];
        for (d, s) in row.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = 255;
        }
    }
}
