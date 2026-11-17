//! Prototipe Lentera dengan Slint.
//!
//! Tujuannya membuktikan tiga hal sebelum Lentera dibangun ulang:
//! 1. jendela output bisa fullscreen di monitor/proyektor yang dipilih,
//! 2. video bisa diputar dengan teks lirik di atasnya,
//! 3. pemakaian RAM dan CPU tetap rendah.
//!
//! Jalankan biasa untuk mencoba manual, atau dengan `--selftest` untuk uji
//! otomatis yang menulis `laporan.json` dan beberapa tangkapan PNG.

slint::include_modules!();

#[cfg(windows)]
mod probe_win;
#[cfg(windows)]
mod video_win;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use slint::winit_030::{WinitWindowAccessor, winit};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use winit::monitor::MonitorHandle;
use winit::window::Fullscreen;

struct Args {
    renderer: String,
    selftest: bool,
    video: Option<PathBuf>,
    monitor: Option<usize>,
    seconds: u64,
    out: PathBuf,
    hw_decode: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        renderer: "femtovg".into(),
        selftest: false,
        hw_decode: true,
        video: None,
        monitor: None,
        seconds: 8,
        out: PathBuf::from("hasil"),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--selftest" => a.selftest = true,
            "--cpu-decode" => a.hw_decode = false,
            "--renderer" => a.renderer = it.next().expect("--renderer butuh nilai"),
            "--video" => a.video = it.next().map(PathBuf::from),
            "--monitor" => a.monitor = it.next().and_then(|s| s.parse().ok()),
            "--seconds" => a.seconds = it.next().and_then(|s| s.parse().ok()).unwrap_or(8),
            "--out" => a.out = it.next().map(PathBuf::from).expect("--out butuh nilai"),
            other => eprintln!("argumen tidak dikenal: {other}"),
        }
    }
    a
}

struct App {
    operator: OperatorWindow,
    output: OutputWindow,
    monitors: RefCell<Vec<MonitorHandle>>,
    primary: RefCell<Option<MonitorHandle>>,
    blackout: Cell<bool>,
    frames_set: Arc<AtomicU64>,
    frames_rendered: Arc<AtomicU64>,
    notifier_ok: Cell<bool>,
    pending: Arc<AtomicBool>,
    hw_decode: bool,
    #[cfg(windows)]
    video: RefCell<Option<video_win::VideoHandle>>,
    #[cfg(windows)]
    stats: RefCell<Arc<video_win::VideoStats>>,
}

impl App {
    fn status(&self, s: impl Into<SharedString>) {
        let s = s.into();
        println!("[status] {s}");
        self.operator.set_status(s);
    }

    fn refresh_monitors(&self) -> bool {
        let found = self.operator.window().with_winit_window(|w| {
            (w.available_monitors().collect::<Vec<_>>(), w.primary_monitor())
        });
        let Some((list, primary)) = found else { return false };
        let labels: Vec<SharedString> = list
            .iter()
            .enumerate()
            .map(|(i, m)| monitor_label(i, m, primary.as_ref()).into())
            .collect();
        self.operator.set_monitors(ModelRc::new(VecModel::from(labels)));
        *self.monitors.borrow_mut() = list;
        *self.primary.borrow_mut() = primary;
        true
    }

    fn show_output(self: &Rc<Self>) {
        let idx = self.operator.get_monitor_index().max(0) as usize;
        if self.output.show().is_err() {
            self.status("Gagal membuka jendela output.");
            return;
        }
        self.operator.set_output_visible(true);
        self.try_place(idx, 0);
    }

    // Jendela winit baru ada setelah `show()` diproses event loop, jadi coba ulang sebentar.
    fn try_place(self: &Rc<Self>, idx: usize, attempt: u32) {
        let Some(m) = self.monitors.borrow().get(idx).cloned() else {
            self.status("Monitor tidak ditemukan.");
            return;
        };
        let placed = self.output.window().with_winit_window(|w| {
            w.set_fullscreen(None);
            w.set_outer_position(m.position());
            w.set_fullscreen(Some(Fullscreen::Borderless(Some(m.clone()))));
        });
        if placed.is_some() {
            self.status(format!("Output fullscreen di monitor {}.", idx + 1));
        } else if attempt < 40 {
            let app = self.clone();
            slint::Timer::single_shot(Duration::from_millis(50), move || app.try_place(idx, attempt + 1));
        } else {
            self.status("Jendela output belum siap; coba lagi.");
        }
    }

    fn hide_output(&self) {
        let _ = self.output.hide();
        self.operator.set_output_visible(false);
    }

    fn go_live(&self) {
        self.output.set_lyric(self.operator.get_lyric_draft());
    }

    fn set_blackout(&self, on: bool) {
        self.blackout.set(on);
        self.output.set_blackout(on);
        self.operator.set_blackout(on);
    }

    #[cfg(windows)]
    fn play_video(&self, path: &Path) {
        self.stop_video();
        if !path.is_file() {
            self.status(format!("File video tidak ada: {}", path.display()));
            return;
        }
        let stats = Arc::new(video_win::VideoStats::default());
        *self.stats.borrow_mut() = stats.clone();
        let weak = self.output.as_weak();
        let pending = self.pending.clone();
        let set = self.frames_set.clone();
        let handle = video_win::start(path.to_path_buf(), self.hw_decode, stats, move |buf| {
            if pending.swap(true, Ordering::SeqCst) {
                return false;
            }
            let (weak, pending, set) = (weak.clone(), pending.clone(), set.clone());
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(o) = weak.upgrade() {
                    o.set_video_frame(slint::Image::from_rgba8(buf));
                    set.fetch_add(1, Ordering::Relaxed);
                }
                pending.store(false, Ordering::SeqCst);
            });
            true
        });
        *self.video.borrow_mut() = Some(handle);
        self.output.set_show_video(true);
        self.operator.set_video_playing(true);
        self.status(format!("Memutar {}", path.display()));
    }

    #[cfg(not(windows))]
    fn play_video(&self, _path: &Path) {
        self.status("Video belum dibuat untuk sistem ini di prototipe (rencana: AVFoundation di macOS).");
    }

    fn stop_video(&self) {
        #[cfg(windows)]
        if let Some(h) = self.video.borrow_mut().take() {
            h.stop();
        }
        self.output.set_show_video(false);
        self.operator.set_video_playing(false);
    }
}

fn monitor_label(i: usize, m: &MonitorHandle, primary: Option<&MonitorHandle>) -> String {
    let (s, p) = (m.size(), m.position());
    format!(
        "{}: {} {}x{} di ({},{}) skala {:.2}{}",
        i + 1,
        m.name().unwrap_or_else(|| "?".into()),
        s.width,
        s.height,
        p.x,
        p.y,
        m.scale_factor(),
        if primary == Some(m) { " - utama" } else { "" }
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(args.renderer.clone())
        .select()?;

    let app = Rc::new(App {
        operator: OperatorWindow::new()?,
        output: OutputWindow::new()?,
        monitors: RefCell::default(),
        primary: RefCell::default(),
        blackout: Cell::new(false),
        frames_set: Arc::default(),
        frames_rendered: Arc::default(),
        notifier_ok: Cell::new(false),
        pending: Arc::default(),
        hw_decode: args.hw_decode,
        #[cfg(windows)]
        video: RefCell::default(),
        #[cfg(windows)]
        stats: RefCell::default(),
    });
    app.operator.set_renderer(args.renderer.clone().into());
    if let Some(v) = &args.video {
        app.operator.set_video_path(v.display().to_string().into());
    }

    let rendered = app.frames_rendered.clone();
    let notifier = app.output.window().set_rendering_notifier(move |state, _| {
        if let slint::RenderingState::AfterRendering = state {
            rendered.fetch_add(1, Ordering::Relaxed);
        }
    });
    app.notifier_ok.set(notifier.is_ok());

    let a = app.clone();
    app.operator.on_show_output(move || a.show_output());
    let a = app.clone();
    app.operator.on_hide_output(move || a.hide_output());
    let a = app.clone();
    app.operator.on_go_live(move || a.go_live());
    let a = app.clone();
    app.operator.on_toggle_blackout(move || a.set_blackout(!a.blackout.get()));
    let a = app.clone();
    app.operator.on_play_video(move || {
        let p = PathBuf::from(a.operator.get_video_path().as_str());
        a.play_video(&p);
    });
    let a = app.clone();
    app.operator.on_stop_video(move || a.stop_video());

    app.operator.show()?;

    let a = app.clone();
    let selftest = args.selftest;
    let args = Rc::new(args);
    slint::Timer::single_shot(Duration::from_millis(400), move || {
        if !a.refresh_monitors() {
            a.status("Tidak bisa membaca daftar monitor.");
        }
        if selftest {
            selftest::run(a.clone(), args.clone());
        }
    });

    slint::run_event_loop()?;
    app.stop_video();
    Ok(())
}

mod selftest {
    //! Uji otomatis: setiap langkah dijadwalkan dengan timer di event loop.

    use super::*;

    #[derive(Default)]
    struct Report(Vec<(String, String)>);

    impl Report {
        fn raw(&mut self, k: &str, v: String) {
            self.0.push((k.into(), v));
        }
        fn str(&mut self, k: &str, v: &str) {
            self.raw(k, format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")));
        }
        fn num(&mut self, k: &str, v: f64) {
            self.raw(k, if v.is_finite() { format!("{v:.3}") } else { "null".into() });
        }
        fn bool(&mut self, k: &str, v: bool) {
            self.raw(k, v.to_string());
        }
        fn json(&self) -> String {
            let body: Vec<String> = self.0.iter().map(|(k, v)| format!("  \"{k}\": {v}")).collect();
            format!("{{\n{}\n}}\n", body.join(",\n"))
        }
    }

    fn after(ms: u64, f: impl FnOnce() + 'static) {
        slint::Timer::single_shot(Duration::from_millis(ms), f);
    }

    pub fn run(app: Rc<App>, args: Rc<Args>) {
        let _ = std::fs::create_dir_all(&args.out);
        let rep = Rc::new(RefCell::new(Report::default()));
        {
            let mut r = rep.borrow_mut();
            r.str("renderer", &args.renderer);
            r.str("os", std::env::consts::OS);
            r.num("cpu_cores", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64);
            r.bool("rendering_notifier_supported", app.notifier_ok.get());
            let labels: Vec<String> = (0..app.monitors.borrow().len())
                .map(|i| monitor_label(i, &app.monitors.borrow()[i], app.primary.borrow().as_ref()))
                .collect();
            r.raw(
                "monitors",
                format!("[{}]", labels.iter().map(|l| format!("\"{l}\"")).collect::<Vec<_>>().join(", ")),
            );
        }

        // Pilih monitor: dari argumen (mulai 1), kalau tidak ada pakai monitor bukan-utama terakhir.
        let n = app.monitors.borrow().len();
        if n == 0 {
            rep.borrow_mut().bool("lulus_monitor_ditemukan", false);
            finish(&app, &args, &rep);
            return;
        }
        let target = args.monitor.map(|m| m.saturating_sub(1).min(n - 1)).unwrap_or_else(|| {
            let primary = app.primary.borrow().clone();
            (0..n).rev().find(|&i| Some(&app.monitors.borrow()[i]) != primary.as_ref()).unwrap_or(0)
        });
        rep.borrow_mut().num("target_monitor", (target + 1) as f64);
        app.operator.set_monitor_index(target as i32);
        app.operator.set_lyric_draft("Baris pertama lirik contoh\nBaris kedua lirik contoh".into());
        app.go_live();
        app.show_output();

        let (a, g, r) = (app.clone(), args.clone(), rep.clone());
        after(1800, move || step_text(a, g, r));
    }

    fn step_text(app: Rc<App>, args: Rc<Args>, rep: Rc<RefCell<Report>>) {
        let target = app.operator.get_monitor_index() as usize;
        let m = app.monitors.borrow()[target].clone();
        let (mp, ms) = (m.position(), m.size());
        let info = app.output.window().with_winit_window(|w| {
            (w.outer_position().ok(), w.outer_size(), w.fullscreen().is_some(), w.current_monitor())
        });
        {
            let mut r = rep.borrow_mut();
            r.raw("monitor_target_px", format!("[{}, {}, {}, {}]", mp.x, mp.y, ms.width, ms.height));
            match info {
                Some((pos, size, fs, cur)) => {
                    let pos_ok = pos.map(|p| p.x == mp.x && p.y == mp.y).unwrap_or(false);
                    let size_ok = size.width == ms.width && size.height == ms.height;
                    if let Some(p) = pos {
                        r.raw("winit_window_px", format!("[{}, {}, {}, {}]", p.x, p.y, size.width, size.height));
                    }
                    r.bool("winit_fullscreen", fs);
                    r.bool("winit_monitor_sama", cur.as_ref() == Some(&m));
                    r.bool("lulus_posisi_winit", pos_ok && size_ok && fs);
                }
                None => r.bool("lulus_posisi_winit", false),
            }
        }

        #[cfg(windows)]
        {
            use raw_window_handle::HasWindowHandle;
            let hwnd = app
                .output
                .window()
                .with_winit_window(|w| w.window_handle().ok().and_then(|h| probe_win::hwnd_from(h.as_raw())))
                .flatten();
            let mut r = rep.borrow_mut();
            if let Some(hwnd) = hwnd {
                if let Some(p) = probe_win::placement(hwnd) {
                    let target_rect = [mp.x, mp.y, mp.x + ms.width as i32, mp.y + ms.height as i32];
                    r.raw("win32_window_rect", format!("{:?}", p.window));
                    r.raw("win32_monitor_rect", format!("{:?}", p.monitor));
                    r.bool("lulus_posisi_win32", p.window == p.monitor && p.monitor == target_rect);
                }
                match probe_win::capture(hwnd) {
                    Some((w, h, px)) => {
                        let bright = fraction(&px, |l| l > 200.0);
                        r.num("teks_piksel_terang_persen", bright * 100.0);
                        r.bool("lulus_teks_terlihat", bright > 0.002);
                        save_png(&args.out.join("1-teks.png"), w, h, &px);
                    }
                    None => r.bool("lulus_teks_terlihat", false),
                }
                if let Some(mem) = probe_win::memory() {
                    r.num("ram_teks_mib", mem.working_set as f64 / 1048576.0);
                    r.num("ram_teks_private_mib", mem.private as f64 / 1048576.0);
                }
            } else {
                r.bool("lulus_posisi_win32", false);
            }
        }

        if let Some(video) = args.video.clone() {
            app.play_video(&video);
            let (a, g, r) = (app.clone(), args.clone(), rep.clone());
            after(1500, move || step_video_measure(a, g, r));
        } else {
            step_blackout(app, args, rep);
        }
    }

    fn step_video_measure(app: Rc<App>, args: Rc<Args>, rep: Rc<RefCell<Report>>) {
        let t0 = Instant::now();
        #[cfg(windows)]
        let cpu0 = probe_win::cpu_seconds().unwrap_or(0.0);
        #[cfg(windows)]
        let dec0 = app.stats.borrow().decoded.load(Ordering::Relaxed);
        let set0 = app.frames_set.load(Ordering::Relaxed);
        let ren0 = app.frames_rendered.load(Ordering::Relaxed);
        let secs = args.seconds;
        let (a, g, r) = (app.clone(), args.clone(), rep.clone());
        after(secs * 1000, move || {
            let dt = t0.elapsed().as_secs_f64();
            {
                let mut rr = r.borrow_mut();
                rr.num("video_detik_diukur", dt);
                rr.num("video_fps_ditayangkan", (a.frames_set.load(Ordering::Relaxed) - set0) as f64 / dt);
                rr.num("video_fps_digambar", (a.frames_rendered.load(Ordering::Relaxed) - ren0) as f64 / dt);
                #[cfg(windows)]
                {
                    let st = a.stats.borrow().clone();
                    let dec = st.decoded.load(Ordering::Relaxed);
                    rr.str("video_decode_diminta", if a.hw_decode { "gpu" } else { "cpu" });
                    rr.bool("video_gpu_siap", st.hw_ready.load(Ordering::Relaxed));
                    rr.bool("video_frame_dari_gpu", st.gpu_surface.load(Ordering::Relaxed));
                    rr.num("video_fps_didecode", (dec - dec0) as f64 / dt);
                    rr.raw(
                        "video_resolusi",
                        format!("\"{}x{}\"", st.width.load(Ordering::Relaxed), st.height.load(Ordering::Relaxed)),
                    );
                    rr.num("video_frame_dibuang_ui_sibuk", st.dropped_busy.load(Ordering::Relaxed) as f64);
                    rr.num("video_frame_terlambat", st.late.load(Ordering::Relaxed) as f64);
                    rr.num(
                        "video_konversi_ms_per_frame",
                        st.convert_ns.load(Ordering::Relaxed) as f64 / 1e6 / dec.max(1) as f64,
                    );
                    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
                    let cpu = probe_win::cpu_seconds().unwrap_or(0.0) - cpu0;
                    rr.num("cpu_video_persen_satu_inti", cpu / dt * 100.0);
                    rr.num("cpu_video_persen_total", cpu / dt / cores * 100.0);
                    if let Some(mem) = probe_win::memory() {
                        rr.num("ram_video_mib", mem.working_set as f64 / 1048576.0);
                        rr.num("ram_video_private_mib", mem.private as f64 / 1048576.0);
                        rr.num("ram_puncak_mib", mem.peak_working_set as f64 / 1048576.0);
                    }
                }
            }
            #[cfg(windows)]
            {
                let first = capture_output(&a);
                let (a2, g2, r2) = (a.clone(), g.clone(), r.clone());
                after(400, move || {
                    let second = capture_output(&a2);
                    {
                        let mut rr = r2.borrow_mut();
                        match (&first, &second) {
                            (Some((w, h, p1)), Some((_, _, p2))) if p1.len() == p2.len() => {
                                let changed = p1
                                    .chunks_exact(4)
                                    .zip(p2.chunks_exact(4))
                                    .filter(|(x, y)| x.iter().zip(y.iter()).any(|(a, b)| a.abs_diff(*b) > 24))
                                    .count() as f64
                                    / (p1.len() / 4) as f64;
                                rr.num("video_piksel_berubah_persen", changed * 100.0);
                                rr.bool("lulus_video_bergerak", changed > 0.01);
                                save_png(&g2.out.join("2-video-teks.png"), *w, *h, p1);
                            }
                            _ => rr.bool("lulus_video_bergerak", false),
                        }
                    }
                    a2.stop_video();
                    step_blackout(a2, g2, r2);
                });
            }
            #[cfg(not(windows))]
            {
                a.stop_video();
                step_blackout(a, g, r);
            }
        });
    }

    #[cfg(windows)]
    fn capture_output(app: &App) -> Option<(u32, u32, Vec<u8>)> {
        use raw_window_handle::HasWindowHandle;
        let hwnd = app
            .output
            .window()
            .with_winit_window(|w| w.window_handle().ok().and_then(|h| probe_win::hwnd_from(h.as_raw())))
            .flatten()?;
        probe_win::capture(hwnd)
    }

    fn step_blackout(app: Rc<App>, args: Rc<Args>, rep: Rc<RefCell<Report>>) {
        app.set_blackout(true);
        let (a, g, r) = (app.clone(), args.clone(), rep.clone());
        after(500, move || {
            #[cfg(windows)]
            {
                let mut rr = r.borrow_mut();
                match capture_output(&a) {
                    Some((w, h, px)) => {
                        let lit = fraction(&px, |l| l > 16.0);
                        rr.num("hitam_piksel_menyala_persen", lit * 100.0);
                        rr.bool("lulus_layar_hitam", lit < 0.0005);
                        save_png(&g.out.join("3-hitam.png"), w, h, &px);
                    }
                    None => rr.bool("lulus_layar_hitam", false),
                }
            }
            a.set_blackout(false);
            finish(&a, &g, &r);
        });
    }

    fn finish(app: &App, args: &Args, rep: &Rc<RefCell<Report>>) {
        let json = rep.borrow().json();
        let path = args.out.join("laporan.json");
        if let Err(e) = std::fs::write(&path, &json) {
            eprintln!("gagal menulis {}: {e}", path.display());
        }
        println!("{json}");
        app.hide_output();
        let _ = slint::quit_event_loop();
    }

    fn fraction(rgba: &[u8], pred: impl Fn(f32) -> bool) -> f64 {
        let total = rgba.len() / 4;
        if total == 0 {
            return 0.0;
        }
        let hit = rgba
            .chunks_exact(4)
            .filter(|p| pred(0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32))
            .count();
        hit as f64 / total as f64
    }

    /// Simpan PNG; gambar lebar diperkecil setengah supaya filenya kecil.
    fn save_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
        let (w2, h2, data) = if w > 1280 {
            let (w2, h2) = (w / 2, h / 2);
            let mut out = Vec::with_capacity((w2 * h2 * 4) as usize);
            for y in 0..h2 {
                for x in 0..w2 {
                    for c in 0..4 {
                        let s: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                            .iter()
                            .map(|(dx, dy)| rgba[(((y * 2 + dy) * w + x * 2 + dx) * 4 + c) as usize] as u32)
                            .sum();
                        out.push((s / 4) as u8);
                    }
                }
            }
            (w2, h2, out)
        } else {
            (w, h, rgba.to_vec())
        };
        let result = std::fs::File::create(path).map_err(|e| e.to_string()).and_then(|f| {
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w2, h2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().map_err(|e| e.to_string())?;
            wr.write_image_data(&data).map_err(|e| e.to_string())
        });
        if let Err(e) = result {
            eprintln!("gagal menyimpan {}: {e}", path.display());
        }
    }
}
