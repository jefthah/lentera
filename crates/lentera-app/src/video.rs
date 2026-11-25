//! Video latar: memutar video di thread sendiri dan mengambil gambar diamnya.
//!
//! Di Windows video dibuka dengan Media Foundation, bawaan Windows, jadi tidak
//! ada codec atau DLL tambahan. Decode memakai GPU (DXVA lewat Direct3D 11)
//! bila tersedia, karena itu jauh lebih ringan untuk PC lemah; bila tidak,
//! decode memakai CPU. Lihat `prototypes/slint-spike/HASIL.md` untuk
//! pengukurannya.
//!
//! Setiap frame diubah ke RGBA lalu diserahkan ke `deliver`. Bila tampilan
//! belum sempat menampilkan frame sebelumnya, `deliver` mengembalikan `false`
//! dan frame itu dibuang, supaya antrean tidak membengkak di PC lemah. Video
//! diputar berulang tanpa suara.
//!
//! macOS belum punya pemutar: video tidak bisa ditambahkan, dan video yang
//! sudah ada di profil tampil sebagai gambar diamnya.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use lentera_core::background::Picture;
use slint::{Rgba8Pixel, SharedPixelBuffer};

/// Video latar bisa ditambahkan dan diputar di sistem ini.
pub const SUPPORTED: bool = cfg!(windows);

/// Frame video yang lebih besar dari ini diperkecil saat didecode. Layar
/// proyektor jarang lebih besar, dan frame 4K butuh empat kali memori.
#[cfg_attr(not(windows), allow(dead_code))]
pub const MAX_FRAME: (u32, u32) = (1920, 1080);

/// Video yang sedang diputar. Menghentikan pemutar tidak menunggu threadnya,
/// kecuali lewat [`Player::stop_and_wait`].
pub struct Player {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Player {
    /// Menghentikan video dan menunggu berkasnya dilepas, misalnya sebelum
    /// berkas itu dihapus. Windows tidak bisa menghapus berkas yang terbuka.
    pub fn stop_and_wait(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Memutar `path` berulang di thread sendiri. `failed` dipanggil sekali bila
/// video tidak bisa diputar sama sekali.
pub fn play<D, F>(path: PathBuf, deliver: D, failed: F) -> Player
where
    D: FnMut(SharedPixelBuffer<Rgba8Pixel>) -> bool + Send + 'static,
    F: FnOnce(String) + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let join = std::thread::Builder::new()
        .name("lentera-video".into())
        .spawn(move || {
            if let Err(e) = imp::run(&path, &flag, deliver) {
                failed(e);
            }
        })
        .ok();
    Player { stop, join }
}

/// Gambar diam video `path`: frame sekitar detik pertama, karena banyak video
/// latar dimulai dari layar gelap. Sekaligus memastikan videonya bisa dibuka.
pub fn poster(path: &Path) -> Result<Picture, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    imp::poster(path).map_err(|e| format!("{name}: {e}"))
}

/// Ukuran frame keluaran: ukuran asli, atau diperkecil supaya muat di
/// [`MAX_FRAME`] dengan perbandingan sisi yang sama. Selalu genap, karena
/// decoder video bekerja per dua piksel.
#[cfg_attr(not(windows), allow(dead_code))]
fn output_size(width: u32, height: u32) -> Option<(u32, u32)> {
    if width <= MAX_FRAME.0 && height <= MAX_FRAME.1 {
        return None;
    }
    let scale = f64::min(
        f64::from(MAX_FRAME.0) / f64::from(width.max(1)),
        f64::from(MAX_FRAME.1) / f64::from(height.max(1)),
    );
    let even = |v: f64| ((v.round() as u32) / 2 * 2).max(2);
    Some((
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    ))
}

#[cfg(windows)]
mod imp {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use lentera_core::background::Picture;
    use slint::{Rgba8Pixel, SharedPixelBuffer};
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::{
        D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_9_3, D3D_FEATURE_LEVEL_10_0,
        D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
    };
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread,
    };
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows::core::{GUID, HSTRING, Interface};

    const FIRST_VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    /// Satuan waktu Media Foundation: 100 ns.
    const SECOND: i64 = 10_000_000;

    /// Media Foundation dan COM untuk thread ini, dilepas saat selesai.
    struct Session;

    impl Session {
        fn start() -> Result<Self, String> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED)
                    .ok()
                    .map_err(|e| e.message())?;
                if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) {
                    CoUninitialize();
                    return Err(format!("Media Foundation tidak tersedia ({})", e.message()));
                }
            }
            Ok(Self)
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
        }
    }

    pub(super) fn run<D>(path: &Path, stop: &AtomicBool, mut deliver: D) -> Result<(), String>
    where
        D: FnMut(SharedPixelBuffer<Rgba8Pixel>) -> bool,
    {
        let _session = Session::start()?;
        // Decode GPU dicoba dulu. Bila gagal (driver lama, codec tidak cocok),
        // video diputar lagi dengan decode CPU.
        if let Ok(gpu) = Gpu::new()
            && (play(path, Some(&gpu), stop, &mut deliver).is_ok() || stop.load(Ordering::SeqCst))
        {
            return Ok(());
        }
        play(path, None, stop, &mut deliver).map_err(|(_, e)| e)
    }

    pub(super) fn poster(path: &Path) -> Result<Picture, String> {
        let _session = Session::start()?;
        let reader = open_reader(path, None).map_err(unsupported)?;
        let mut fmt = current_format(&reader).map_err(unsupported)?;
        // Lompat ke detik pertama; bila video lebih pendek, ambil frame terakhir
        // yang terbaca.
        let seeked = unsafe {
            reader
                .SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from(SECOND))
                .is_ok()
        };
        let mut last = None;
        for _ in 0..90 {
            let (flags, ts, sample) = read(&reader).map_err(unsupported)?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                fmt = current_format(&reader).map_err(unsupported)?;
            }
            let Some(sample) = sample else { continue };
            last = Some(unsafe { to_rgba(&sample, &fmt) }.map_err(unsupported)?);
            if !seeked || ts >= SECOND {
                break;
            }
        }
        let Some(frame) = last else {
            return Err("video tidak berisi gambar.".into());
        };
        let rgb = frame
            .as_slice()
            .iter()
            .flat_map(|px| [px.r, px.g, px.b])
            .collect();
        Ok(Picture {
            width: frame.width(),
            height: frame.height(),
            rgb,
        })
    }

    fn unsupported(_: windows::core::Error) -> String {
        "video tidak bisa dibuka, mungkin rusak atau formatnya belum didukung Windows. Gunakan video MP4 (H.264) atau WMV.".into()
    }

    /// Perangkat Direct3D 11 untuk decode video di GPU.
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
                let device = device.ok_or_else(windows::core::Error::empty)?;
                // Media Foundation memakai perangkat ini dari beberapa thread.
                if let Some(mt) = context.and_then(|c| c.cast::<ID3D11Multithread>().ok()) {
                    let _ = mt.SetMultithreadProtected(true);
                }
                let mut token = 0u32;
                let mut manager: Option<IMFDXGIDeviceManager> = None;
                MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
                let manager = manager.ok_or_else(windows::core::Error::empty)?;
                manager.ResetDevice(&device, token)?;
                Ok(Gpu {
                    _device: device,
                    manager,
                })
            }
        }
    }

    /// Ukuran frame dari decoder, plus bagian yang benar-benar ditampilkan.
    /// Video 1080p H.264 biasanya didecode 1088 baris; 8 baris terakhir
    /// dibuang.
    struct Format {
        height: u32,
        stride: i32,
        crop_x: u32,
        crop_y: u32,
        show_w: u32,
        show_h: u32,
    }

    fn open_reader(path: &Path, gpu: Option<&Gpu>) -> windows::core::Result<IMFSourceReader> {
        unsafe {
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 3)?;
            let attrs = attrs.ok_or_else(windows::core::Error::empty)?;
            if let Some(gpu) = gpu {
                attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &gpu.manager)?;
                attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            }
            // Pemroses video Media Foundation mengubah YUV ke RGB32 dan, bila
            // perlu, memperkecil frame: di GPU bila ada perangkat Direct3D,
            // selain itu di CPU.
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
            let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attrs)?;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            reader.SetStreamSelection(FIRST_VIDEO, true)?;
            let native = reader.GetNativeMediaType(FIRST_VIDEO, 0)?;
            let size = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
            let resize = super::output_size((size >> 32) as u32, (size & 0xffff_ffff) as u32);
            let rgb = |frame: Option<(u32, u32)>| -> windows::core::Result<IMFMediaType> {
                let mt = MFCreateMediaType()?;
                mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
                if let Some((w, h)) = frame {
                    mt.SetUINT64(&MF_MT_FRAME_SIZE, (u64::from(w) << 32) | u64::from(h))?;
                }
                Ok(mt)
            };
            // Bila pemroses video tidak bisa memperkecil, frame dipakai apa
            // adanya.
            if resize.is_none()
                || reader
                    .SetCurrentMediaType(FIRST_VIDEO, None, &rgb(resize)?)
                    .is_err()
            {
                reader.SetCurrentMediaType(FIRST_VIDEO, None, &rgb(None)?)?;
            }
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
            let (crop_x, crop_y, show_w, show_h) =
                match mt.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, bytes, None) {
                    Ok(()) if area.Area.cx > 0 && area.Area.cy > 0 => {
                        let x = (area.OffsetX.value.max(0) as u32).min(width);
                        let y = (area.OffsetY.value.max(0) as u32).min(height);
                        (
                            x,
                            y,
                            (area.Area.cx as u32).min(width - x),
                            (area.Area.cy as u32).min(height - y),
                        )
                    }
                    _ => (0, 0, width, height),
                };
            Ok(Format {
                height,
                stride,
                crop_x,
                crop_y,
                show_w,
                show_h,
            })
        }
    }

    fn read(reader: &IMFSourceReader) -> windows::core::Result<(u32, i64, Option<IMFSample>)> {
        let mut flags = 0u32;
        let mut ts = 0i64;
        let mut sample: Option<IMFSample> = None;
        unsafe {
            reader.ReadSample(
                FIRST_VIDEO,
                0,
                None,
                Some(&mut flags),
                Some(&mut ts),
                Some(&mut sample),
            )?;
        }
        Ok((flags, ts, sample))
    }

    /// Memutar video berulang sampai `stop`. Galat dikembalikan bersama jumlah
    /// frame yang sudah tampil.
    fn play<D>(
        path: &Path,
        gpu: Option<&Gpu>,
        stop: &AtomicBool,
        deliver: &mut D,
    ) -> Result<(), (u64, String)>
    where
        D: FnMut(SharedPixelBuffer<Rgba8Pixel>) -> bool,
    {
        let mut frames = 0u64;
        let fail = |frames: u64| move |e: windows::core::Error| (frames, unsupported(e));
        let reader = open_reader(path, gpu).map_err(fail(0))?;
        let mut fmt = current_format(&reader).map_err(fail(0))?;
        let mut start = Instant::now();
        let mut first_ts: Option<i64> = None;
        while !stop.load(Ordering::SeqCst) {
            let (flags, ts, sample) = read(&reader).map_err(fail(frames))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                if frames == 0 {
                    return Err((0, "video tidak berisi gambar.".into()));
                }
                // Ulang dari awal tanpa membuka berkas lagi.
                unsafe {
                    reader
                        .SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from(0i64))
                        .map_err(fail(frames))?;
                }
                start = Instant::now();
                first_ts = None;
                continue;
            }
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                fmt = current_format(&reader).map_err(fail(frames))?;
            }
            let Some(sample) = sample else { continue };
            let frame = unsafe { to_rgba(&sample, &fmt) }.map_err(fail(frames))?;
            // Tunggu sampai waktu tayang frame ini.
            let base = *first_ts.get_or_insert(ts);
            let due = start + Duration::from_nanos(((ts - base).max(0) as u64) * 100);
            let now = Instant::now();
            if due > now {
                std::thread::sleep(due - now);
            }
            if stop.load(Ordering::SeqCst) {
                break;
            }
            deliver(frame);
            frames += 1;
        }
        Ok(())
    }

    unsafe fn to_rgba(
        sample: &IMFSample,
        fmt: &Format,
    ) -> windows::core::Result<SharedPixelBuffer<Rgba8Pixel>> {
        let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(fmt.show_w, fmt.show_h);
        let dst = out.make_mut_bytes();
        let buffer = unsafe { sample.GetBufferByIndex(0)? };
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

    /// Menyalin bagian yang ditampilkan dari baris BGRX ke RGBA.
    unsafe fn copy_rows(scan0: *const u8, pitch: isize, fmt: &Format, dst: &mut [u8]) {
        let (w, h) = (fmt.show_w as usize, fmt.show_h as usize);
        let x0 = fmt.crop_x as usize * 4;
        for y in 0..h {
            let row_start = unsafe {
                scan0
                    .offset(pitch * (y + fmt.crop_y as usize) as isize)
                    .add(x0)
            };
            let src = unsafe { std::slice::from_raw_parts(row_start, w * 4) };
            let row = &mut dst[y * w * 4..(y + 1) * w * 4];
            for (d, s) in row
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(src.as_chunks::<4>().0)
            {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = 255;
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    use lentera_core::background::Picture;
    use slint::{Rgba8Pixel, SharedPixelBuffer};

    const NOT_YET: &str = "video latar belum bisa diputar di macOS.";

    pub(super) fn run<D>(_path: &Path, _stop: &AtomicBool, _deliver: D) -> Result<(), String>
    where
        D: FnMut(SharedPixelBuffer<Rgba8Pixel>) -> bool,
    {
        Err(NOT_YET.into())
    }

    pub(super) fn poster(_path: &Path) -> Result<Picture, String> {
        Err(NOT_YET.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_frames_are_shrunk_to_fit() {
        assert_eq!(output_size(1920, 1080), None);
        assert_eq!(output_size(1280, 720), None);
        assert_eq!(output_size(3840, 2160), Some((1920, 1080)));
        assert_eq!(output_size(4096, 2160), Some((1920, 1012)));
        // Video tegak (potret) dibatasi tingginya.
        assert_eq!(output_size(1080, 1920), Some((608, 1080)));
    }

    /// Membuka video sungguhan, yang tidak ada di repositori:
    /// `LENTERA_TEST_VIDEO=<berkas>[;<berkas>…] cargo test -p lentera-app real_video -- --ignored --nocapture`
    #[test]
    #[ignore = "butuh video uji di LENTERA_TEST_VIDEO"]
    fn real_video_gives_poster_and_frames() {
        use std::sync::atomic::AtomicU64;
        use std::time::{Duration, Instant};
        let list = std::env::var("LENTERA_TEST_VIDEO").expect("LENTERA_TEST_VIDEO");
        for file in list.split(';').filter(|f| !f.is_empty()) {
            let path = PathBuf::from(file);
            let t0 = Instant::now();
            match poster(&path) {
                Ok(p) => println!(
                    "{file}: gambar diam {}×{} dalam {} ms",
                    p.width,
                    p.height,
                    t0.elapsed().as_millis()
                ),
                Err(e) => {
                    println!("{file}: gambar diam gagal: {e}");
                    continue;
                }
            }
            let frames = Arc::new(AtomicU64::new(0));
            let size = Arc::new(std::sync::Mutex::new((0, 0)));
            let (f, s) = (frames.clone(), size.clone());
            let error = Arc::new(std::sync::Mutex::new(None));
            let e2 = error.clone();
            let player = play(
                path.clone(),
                move |frame| {
                    f.fetch_add(1, Ordering::Relaxed);
                    *s.lock().unwrap() = (frame.width(), frame.height());
                    true
                },
                move |e| *e2.lock().unwrap() = Some(e),
            );
            std::thread::sleep(Duration::from_secs(4));
            player.stop_and_wait();
            println!(
                "{file}: {} frame dalam 4 detik, ukuran {:?}, galat {:?}",
                frames.load(Ordering::Relaxed),
                size.lock().unwrap(),
                error.lock().unwrap()
            );
        }
    }
}
