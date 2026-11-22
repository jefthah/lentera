//! Gambar latar layar, disimpan per profil.
//!
//! ```text
//! profiles/<id profil>/backgrounds/<nama>.jpg          gambar latar
//! profiles/<id profil>/backgrounds/thumbs/<nama>.jpg   gambar kecil 192×108 untuk daftar
//! ```
//!
//! Berkas asli tidak diubah. Lentera menyimpan salinan JPEG yang sudah diputar
//! sesuai EXIF dan diperkecil secukupnya untuk menutup layar 1920×1080, supaya
//! cepat dibuka dan hemat RAM di komputer yang lemah. Yang disimpan di pustaka
//! hanya nama berkasnya; nama itu selalu diperiksa sebelum dipakai, sehingga
//! tidak bisa menunjuk ke luar folder latar.

use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageError, ImageReader, Limits, RgbImage,
    codecs::jpeg::JpegEncoder,
    error::LimitErrorKind,
    imageops::{self, FilterType},
    metadata::Orientation,
};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// Folder latar di dalam folder profil.
pub const BACKGROUNDS_DIR: &str = "backgrounds";
/// Folder gambar kecil di dalam folder latar.
const THUMBS_DIR: &str = "thumbs";
/// Layar yang harus bisa ditutup penuh oleh gambar tanpa terlihat kabur.
pub const SCREEN: (u32, u32) = (1920, 1080);
/// Sisi terpanjang yang disimpan, untuk gambar panorama yang sangat lebar.
const MAX_SIDE: u32 = 3840;
/// Ukuran gambar kecil di daftar latar (16:9).
pub const THUMB: (u32, u32) = (192, 108);
/// Ekstensi berkas yang bisa ditambahkan. `.jfif` adalah JPEG, misalnya
/// gambar yang disimpan dari WhatsApp.
pub const EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "jfif", "png"];
/// Berkas sumber yang lebih besar dari ini ditolak.
pub const MAX_SOURCE_BYTES: u64 = 40 * 1024 * 1024;
/// Batas memori saat membuka gambar sumber (foto 24 MP butuh ±72 MB).
const MAX_DECODE_BYTES: u64 = 320 * 1024 * 1024;
const QUALITY: u8 = 88;
const THUMB_QUALITY: u8 = 80;
/// Panjang maksimum bagian nama sebelum ".jpg".
const STEM_MAX: usize = 60;
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Gambar yang sudah dibuka: piksel RGB 8 bit, baris demi baris.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Picture {
    fn from_image(image: RgbImage) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            rgb: image.into_raw(),
        }
    }
}

/// Hasil [`add`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Added {
    /// Gambar baru disimpan dengan nama ini.
    New(String),
    /// Gambar yang sama persis sudah ada dengan nama ini; tidak ada yang ditulis.
    Existing(String),
}

impl Added {
    pub fn name(&self) -> &str {
        match self {
            Self::New(name) | Self::Existing(name) => name,
        }
    }
}

/// Folder latar milik profil.
pub fn folder(profile_dir: &Path) -> PathBuf {
    profile_dir.join(BACKGROUNDS_DIR)
}

/// Nama semua gambar latar profil, urut abjad tanpa membedakan huruf besar.
/// Folder yang belum ada berarti belum ada latar.
pub fn list(profile_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder(profile_dir)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_name(name))
        .collect();
    names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
    names
}

/// Menambahkan salinan gambar `source` ke latar profil. Gambar yang sama
/// persis dengan latar yang sudah ada tidak disimpan dua kali.
pub fn add(profile_dir: &Path, source: &Path) -> Result<Added, String> {
    let size = fs::metadata(source)
        .map_err(|e| format!("{}: {e}", display_name(source)))?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err(format!(
            "{} lebih besar dari {} MB.",
            display_name(source),
            MAX_SOURCE_BYTES / 1024 / 1024
        ));
    }
    let image = decode(source).map_err(|e| format!("{}: {e}", display_name(source)))?;
    let image = fit(image);
    let bytes = encode(&image, QUALITY)?;
    let dir = folder(profile_dir);
    if let Some(name) = same_file(&dir, &bytes) {
        return Ok(Added::Existing(name));
    }
    fs::create_dir_all(dir.join(THUMBS_DIR)).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    let name = free_name(&dir, &clean_stem(&stem));
    write_new(&dir.join(&name), &bytes)?;
    // Gambar kecil boleh gagal; [`thumbnail`] membuatnya lagi bila perlu.
    if let Ok(thumb) = encode(&thumb_of(&image), THUMB_QUALITY) {
        let _ = write_new(&thumb_path(&dir, &name), &thumb);
    }
    Ok(Added::New(name))
}

/// Membuka gambar latar untuk ditayangkan.
pub fn load(profile_dir: &Path, name: &str) -> Result<Picture, String> {
    check_name(name)?;
    let path = folder(profile_dir).join(name);
    let image = decode(&path).map_err(|e| format!("Latar {name}: {e}"))?;
    // Berkas yang ditaruh sendiri di folder latar bisa saja sangat besar.
    Ok(Picture::from_image(fit(image)))
}

/// Gambar kecil untuk daftar latar. Bila belum ada atau rusak, dibuat ulang
/// dari gambar latarnya.
pub fn thumbnail(profile_dir: &Path, name: &str) -> Result<Picture, String> {
    check_name(name)?;
    let dir = folder(profile_dir);
    let path = thumb_path(&dir, name);
    if let Ok(image) = decode(&path)
        && (image.width(), image.height()) == THUMB
    {
        return Ok(Picture::from_image(image));
    }
    let image = decode(&dir.join(name)).map_err(|e| format!("Latar {name}: {e}"))?;
    let thumb = thumb_of(&fit(image));
    if let Ok(bytes) = encode(&thumb, THUMB_QUALITY) {
        let _ = fs::create_dir_all(dir.join(THUMBS_DIR)).and_then(|()| fs::write(&path, bytes));
    }
    Ok(Picture::from_image(thumb))
}

/// Menghapus salinan latar dari profil. Berkas asli di tempat lain tidak
/// tersentuh.
pub fn remove(profile_dir: &Path, name: &str) -> Result<(), String> {
    check_name(name)?;
    let dir = folder(profile_dir);
    match fs::remove_file(dir.join(name)) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Latar {name} tidak bisa dihapus: {e}")),
    }
    let _ = fs::remove_file(thumb_path(&dir, name));
    Ok(())
}

/// Menyalin latar `name` dari satu profil ke profil lain, misalnya saat
/// membuat profil baru. Mengembalikan nama di profil tujuan.
pub fn copy(from_profile: &Path, to_profile: &Path, name: &str) -> Result<String, String> {
    check_name(name)?;
    let from = folder(from_profile);
    let bytes = fs::read(from.join(name)).map_err(|e| format!("Latar {name}: {e}"))?;
    let to = folder(to_profile);
    if let Some(existing) = same_file(&to, &bytes) {
        return Ok(existing);
    }
    fs::create_dir_all(to.join(THUMBS_DIR)).map_err(|e| format!("{}: {e}", to.display()))?;
    let stem = name.strip_suffix(".jpg").unwrap_or(name);
    let target = free_name(&to, stem);
    write_new(&to.join(&target), &bytes)?;
    if let Ok(thumb) = fs::read(thumb_path(&from, name)) {
        let _ = write_new(&thumb_path(&to, &target), &thumb);
    }
    Ok(target)
}

/// Apakah berkas ini tampak seperti gambar yang bisa ditambahkan, menurut
/// ekstensinya.
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| EXTENSIONS.iter().any(|ok| ext.eq_ignore_ascii_case(ok)))
}

fn thumb_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(THUMBS_DIR).join(name)
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Nama latar yang sah: satu nama berkas `.jpg` di folder latar, tanpa
/// pemisah folder, tanpa titik di depan, dan tanpa karakter kontrol.
pub(crate) fn valid_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".jpg") else {
        return false;
    };
    !stem.trim().is_empty()
        && stem.chars().count() <= STEM_MAX + 8
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

fn check_name(name: &str) -> Result<(), String> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(format!("Nama latar tidak sah: {name:?}"))
    }
}

/// Nama berkas dari nama gambar asli: karakter yang tidak boleh ada di nama
/// berkas diganti tanda hubung.
fn clean_stem(stem: &str) -> String {
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .take(STEM_MAX)
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c.is_whitespace() || c == '.');
    let mut cleaned = cleaned.replace("..", ".");
    // "foto.jpg.jpeg" sudah kehilangan ".jpeg"; ".jpg" yang tersisa juga
    // dibuang supaya tidak tampil di nama latar.
    while let Some(dot) = cleaned.rfind('.') {
        if !EXTENSIONS
            .iter()
            .any(|ext| cleaned[dot + 1..].eq_ignore_ascii_case(ext))
        {
            break;
        }
        cleaned.truncate(dot);
        let kept = cleaned
            .trim_end_matches(|c: char| c.is_whitespace() || c == '.')
            .len();
        cleaned.truncate(kept);
    }
    if cleaned.is_empty() {
        return "latar".into();
    }
    if RESERVED.iter().any(|r| cleaned.eq_ignore_ascii_case(r)) {
        return format!("{cleaned}-latar");
    }
    cleaned
}

/// `stem.jpg`, atau `stem (2).jpg`, `stem (3).jpg`, … bila sudah dipakai.
fn free_name(dir: &Path, stem: &str) -> String {
    let mut name = format!("{stem}.jpg");
    let mut number = 2;
    while dir.join(&name).exists() {
        name = format!("{stem} ({number}).jpg");
        number += 1;
    }
    name
}

/// Latar yang isinya sama persis dengan `bytes`. Panjang berkas dibandingkan
/// dulu, jadi biasanya tidak ada berkas yang perlu dibaca.
fn same_file(dir: &Path, bytes: &[u8]) -> Option<String> {
    let profile = dir.parent()?;
    list(profile).into_iter().find(|name| {
        let path = dir.join(name);
        fs::metadata(&path).is_ok_and(|meta| meta.len() == bytes.len() as u64)
            && fs::read(&path).is_ok_and(|existing| existing == bytes)
    })
}

/// Menulis lewat berkas sementara, supaya tidak pernah ada latar setengah jadi.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&temp, bytes)
        .and_then(|()| fs::rename(&temp, path))
        .map_err(|e| {
            let _ = fs::remove_file(&temp);
            format!("{}: {e}", path.display())
        })
}

fn decode(path: &Path) -> Result<RgbImage, String> {
    let mut reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(describe)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(describe)?;
    image.apply_orientation(orientation);
    Ok(flatten(image))
}

fn describe(error: ImageError) -> String {
    match error {
        ImageError::Unsupported(_) => "format gambar belum didukung. Gunakan JPG atau PNG.".into(),
        ImageError::Limits(limit) if matches!(limit.kind(), LimitErrorKind::InsufficientMemory) => {
            "gambar terlalu besar untuk dibuka.".into()
        }
        ImageError::Decoding(_) => "berkas gambar rusak atau tidak lengkap.".into(),
        other => other.to_string(),
    }
}

/// Bagian transparan digabung di atas hitam, warna latar layar yang paling
/// aman.
fn flatten(image: DynamicImage) -> RgbImage {
    if !image.color().has_alpha() {
        return image.into_rgb8();
    }
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = RgbImage::new(width, height);
    for (out, pixel) in rgb.pixels_mut().zip(rgba.pixels()) {
        let [r, g, b, a] = pixel.0;
        let blend = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
        out.0 = [blend(r), blend(g), blend(b)];
    }
    rgb
}

/// Faktor perkecil supaya gambar masih menutup [`SCREEN`] penuh, tanpa pernah
/// memperbesar, dan sisi terpanjang tidak lebih dari [`MAX_SIDE`].
fn fit_scale(width: u32, height: u32) -> f64 {
    let (width, height) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let cover = f64::max(f64::from(SCREEN.0) / width, f64::from(SCREEN.1) / height);
    let side = f64::from(MAX_SIDE) / width.max(height);
    cover.min(side).min(1.0)
}

fn fit(image: RgbImage) -> RgbImage {
    let scale = fit_scale(image.width(), image.height());
    if scale >= 1.0 {
        return image;
    }
    let width = ((f64::from(image.width()) * scale).round() as u32).max(1);
    let height = ((f64::from(image.height()) * scale).round() as u32).max(1);
    imageops::thumbnail(&image, width, height)
}

/// Potongan tengah 16:9 yang diperkecil ke [`THUMB`], sama seperti gambar
/// yang terlihat di layar 16:9.
fn thumb_of(image: &RgbImage) -> RgbImage {
    let (width, height) = image.dimensions();
    let (tw, th) = (u64::from(THUMB.0), u64::from(THUMB.1));
    let (crop_w, crop_h) = if u64::from(width) * th > u64::from(height) * tw {
        ((u64::from(height) * tw / th) as u32, height)
    } else {
        (width, (u64::from(width) * th / tw) as u32)
    };
    let (crop_w, crop_h) = (crop_w.max(1), crop_h.max(1));
    let crop = imageops::crop_imm(
        image,
        (width - crop_w) / 2,
        (height - crop_h) / 2,
        crop_w,
        crop_h,
    )
    .to_image();
    if crop_w >= THUMB.0 && crop_h >= THUMB.1 {
        imageops::thumbnail(&crop, THUMB.0, THUMB.1)
    } else {
        imageops::resize(&crop, THUMB.0, THUMB.1, FilterType::Triangle)
    }
}

fn encode(image: &RgbImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("Gambar tidak bisa disimpan: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb, RgbaImage};

    /// Folder sementara yang dihapus saat selesai.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("lentera-latar-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Gambar uji bergradasi; `seed` membuat isinya berbeda.
    fn gradient(width: u32, height: u32, seed: u8) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            Rgb([
                (x * 255 / width.max(1)) as u8,
                (y * 255 / height.max(1)) as u8,
                seed,
            ])
        })
    }

    fn save(dir: &Path, file: &str, image: &RgbImage) -> PathBuf {
        let path = dir.join(file);
        let format = ImageFormat::from_path(&path).unwrap();
        image.save_with_format(&path, format).unwrap();
        path
    }

    #[test]
    fn large_photo_is_shrunk_to_cover_the_screen() {
        let dir = TempDir::new("besar");
        let profile = dir.0.join("profil");
        let source = save(&dir.0, "Salib senja.jpg", &gradient(2400, 1600, 10));
        let added = add(&profile, &source).unwrap();
        assert_eq!(added, Added::New("Salib senja.jpg".into()));
        let picture = load(&profile, "Salib senja.jpg").unwrap();
        // 2400×1600 × 0,8 = 1920×1280: lebar pas, tinggi masih menutup 1080.
        assert_eq!((picture.width, picture.height), (1920, 1280));
        assert_eq!(picture.rgb.len(), 1920 * 1280 * 3);
        let thumb = thumbnail(&profile, "Salib senja.jpg").unwrap();
        assert_eq!((thumb.width, thumb.height), THUMB);
        assert!(
            folder(&profile)
                .join("thumbs")
                .join("Salib senja.jpg")
                .is_file()
        );
        // Berkas asli tidak berubah.
        assert_eq!(
            ImageReader::open(&source)
                .unwrap()
                .into_dimensions()
                .unwrap(),
            (2400, 1600)
        );
    }

    #[test]
    fn small_and_tall_images_are_never_enlarged() {
        let dir = TempDir::new("kecil");
        let profile = dir.0.join("profil");
        let small = save(&dir.0, "kecil.png", &gradient(640, 360, 1));
        add(&profile, &small).unwrap();
        let picture = load(&profile, "kecil.jpg").unwrap();
        assert_eq!((picture.width, picture.height), (640, 360));
        // Potret: lebar 1920 menentukan, tinggi ikut.
        let tall = save(&dir.0, "potret.jpg", &gradient(2100, 2500, 2));
        add(&profile, &tall).unwrap();
        let picture = load(&profile, "potret.jpg").unwrap();
        assert_eq!((picture.width, picture.height), (1920, 2286));
        // Panorama sangat lebar dibatasi sisi terpanjangnya.
        let wide = save(&dir.0, "panorama.jpg", &gradient(5000, 600, 3));
        add(&profile, &wide).unwrap();
        let picture = load(&profile, "panorama.jpg").unwrap();
        assert_eq!((picture.width, picture.height), (3840, 461));
        assert_eq!(list(&profile), ["kecil.jpg", "panorama.jpg", "potret.jpg"]);
    }

    #[test]
    fn transparent_png_is_flattened_on_black() {
        let dir = TempDir::new("transparan");
        let profile = dir.0.join("profil");
        let mut rgba = RgbaImage::from_pixel(64, 36, image::Rgba([255, 255, 255, 0]));
        rgba.put_pixel(0, 0, image::Rgba([200, 100, 50, 255]));
        let path = dir.0.join("logo.png");
        rgba.save(&path).unwrap();
        add(&profile, &path).unwrap();
        let picture = load(&profile, "logo.jpg").unwrap();
        // JPEG tidak persis, tetapi sudut kanan bawah yang transparan harus gelap.
        let last = picture.rgb.len() - 3;
        assert!(
            picture.rgb[last..].iter().all(|&c| c < 24),
            "{:?}",
            &picture.rgb[last..]
        );
    }

    #[test]
    fn same_image_is_stored_once_and_names_do_not_clash() {
        let dir = TempDir::new("ganda");
        let profile = dir.0.join("profil");
        let first = save(&dir.0, "latar.jpg", &gradient(800, 450, 5));
        assert_eq!(
            add(&profile, &first).unwrap(),
            Added::New("latar.jpg".into())
        );
        assert_eq!(
            add(&profile, &first).unwrap(),
            Added::Existing("latar.jpg".into())
        );
        // Isi sama, nama lain: tetap satu berkas.
        let copy = dir.0.join("salinan.jpg");
        fs::copy(&first, &copy).unwrap();
        assert_eq!(
            add(&profile, &copy).unwrap(),
            Added::Existing("latar.jpg".into())
        );
        // Nama sama, isi lain: diberi nomor.
        let other_dir = dir.0.join("lain");
        fs::create_dir_all(&other_dir).unwrap();
        let other = save(&other_dir, "latar.jpg", &gradient(800, 450, 99));
        assert_eq!(
            add(&profile, &other).unwrap(),
            Added::New("latar (2).jpg".into())
        );
        assert_eq!(list(&profile), ["latar (2).jpg", "latar.jpg"]);
    }

    #[test]
    fn file_names_are_cleaned() {
        assert_eq!(clean_stem("Salib: senja?"), "Salib- senja-");
        assert_eq!(clean_stem("  ..  "), "latar");
        assert_eq!(clean_stem("con"), "con-latar");
        assert_eq!(clean_stem("a..b"), "a.b");
        assert_eq!(clean_stem(&"x".repeat(100)).len(), STEM_MAX);
        assert_eq!(clean_stem("Kasih Tuhan (2)"), "Kasih Tuhan (2)");
        assert_eq!(clean_stem("Paskah.jpg"), "Paskah");
        assert_eq!(clean_stem("Paskah .JPG.png"), "Paskah");
        assert_eq!(clean_stem("v1.2"), "v1.2");
        assert_eq!(clean_stem("x.jpg"), "x");
        assert_eq!(clean_stem(".jpg"), "jpg");
    }

    #[test]
    fn names_outside_the_folder_are_refused() {
        let dir = TempDir::new("nama");
        let profile = dir.0.join("profil");
        for name in [
            "../library.db",
            "..\\x.jpg",
            "a/b.jpg",
            ".tersembunyi.jpg",
            "",
            "x.png",
            "C:x.jpg",
        ] {
            assert!(load(&profile, name).is_err(), "{name}");
            assert!(remove(&profile, name).is_err(), "{name}");
        }
        assert!(load(&profile, "tidak-ada.jpg").is_err());
        // Menghapus latar yang tidak ada bukan galat.
        assert!(remove(&profile, "tidak-ada.jpg").is_ok());
    }

    #[test]
    fn broken_and_unsupported_files_give_clear_errors() {
        let dir = TempDir::new("rusak");
        let profile = dir.0.join("profil");
        let text = dir.0.join("catatan.jpg");
        fs::write(&text, "bukan gambar").unwrap();
        let error = add(&profile, &text).unwrap_err();
        assert!(error.starts_with("catatan.jpg: "), "{error}");
        let gif = dir.0.join("animasi.gif");
        fs::write(&gif, b"GIF89a\x01\x00\x01\x00\x00\x00\x00;").unwrap();
        let error = add(&profile, &gif).unwrap_err();
        assert!(error.contains("JPG atau PNG"), "{error}");
        assert!(list(&profile).is_empty());
        assert!(!is_supported(&gif));
        assert!(is_supported(Path::new("A.JPEG")));
        assert!(is_supported(Path::new("dari whatsapp.jfif")));
    }

    #[test]
    fn missing_thumbnail_is_made_again() {
        let dir = TempDir::new("thumb");
        let profile = dir.0.join("profil");
        let source = save(&dir.0, "pagi.jpg", &gradient(1280, 720, 3));
        add(&profile, &source).unwrap();
        let thumb = folder(&profile).join("thumbs").join("pagi.jpg");
        fs::remove_file(&thumb).unwrap();
        let picture = thumbnail(&profile, "pagi.jpg").unwrap();
        assert_eq!((picture.width, picture.height), THUMB);
        assert!(thumb.is_file());
    }

    #[test]
    fn copy_and_remove() {
        let dir = TempDir::new("salin");
        let (one, two) = (dir.0.join("satu"), dir.0.join("dua"));
        let source = save(&dir.0, "malam.jpg", &gradient(1000, 600, 4));
        add(&one, &source).unwrap();
        assert_eq!(copy(&one, &two, "malam.jpg").unwrap(), "malam.jpg");
        // Menyalin lagi tidak membuat berkas ganda.
        assert_eq!(copy(&one, &two, "malam.jpg").unwrap(), "malam.jpg");
        assert_eq!(list(&two), ["malam.jpg"]);
        assert!(folder(&two).join("thumbs").join("malam.jpg").is_file());
        remove(&two, "malam.jpg").unwrap();
        assert!(list(&two).is_empty());
        assert!(!folder(&two).join("thumbs").join("malam.jpg").exists());
        assert_eq!(list(&one), ["malam.jpg"]);
    }
}
