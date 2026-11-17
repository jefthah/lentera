# Lentera 0.2 — presentasi ibadah ringan

Aplikasi presentasi ibadah berbasis Rust dan Slint untuk lirik, ayat, dan proyektor, untuk Windows dan macOS. Sasaran pengembangannya komputer dengan RAM 2–4 GB; ini sasaran, bukan hasil uji pada semua komputer lama.

## Memasang dan memperbarui

1. Unduh **LenteraApp-win-Setup.exe** dari halaman rilis GitHub, lalu jalankan. Tidak perlu hak admin. Karena aplikasi belum bertanda tangan digital, Windows mungkin menampilkan "Windows protected your PC": pilih **More info → Run anyway**.
2. Buka Lentera dari shortcut **Lentera** di Desktop atau Start Menu.
3. Saat dibuka dan tersambung ke internet, Lentera memeriksa versi baru dan mengunduhnya di latar belakang. Bila sudah siap, muncul tombol **Perbarui ke …** di kanan bawah.
   - Biarkan saja: pembaruan dipasang otomatis saat Lentera ditutup.
   - Atau klik tombolnya, lalu pilih **Yes**: Lentera ditutup, diperbarui, dan dibuka kembali. Proyektor ikut tertutup sebentar, jadi lakukan di luar ibadah.
4. Untuk menghapus: Settings → Apps → **Lentera** → Uninstall. Data di `%LOCALAPPDATA%\Lentera` tidak ikut terhapus.

Pengguna 0.1.1 menerima 0.2.0 lewat pembaruan otomatis di atas. Pengguna 0.1.0 (ZIP lama) cukup memasang installer sekali; data lama langsung terbaca.

Versi macOS belum punya installer; bangun dari kode sumber (lihat **Untuk pengembang**).

## Mengenal layar

- **Bilah atas:** kolom pencarian, tombol **Buka / Tutup proyektor**, pilihan monitor proyektor, menu **Impor**, dan tombol **ⓘ** (Tentang Lentera).
- **Kiri:** **Susunan ibadah** di atas, **Pustaka** di bawah. Filter **Semua / Lagu / Ayat** menyaring pustaka.
- **Tengah:** semua slide materi terpilih. Slide berbingkai kuning sedang dipratinjau, slide berbingkai merah sedang tayang.
- **Kanan:** **Pratinjau**, tombol **Tayangkan**, layar **Tayang** (yang sedang dilihat jemaat), tombol kontrol, dan **Latar layar**.
- **Bilah bawah:** pesan status, tombol cepat, dan tombol pembaruan bila ada.

## Mulai memakai

1. Tekan **Materi baru** di Pustaka (atau Ctrl+N). Bila filter **Ayat** aktif, materi baru berjenis ayat; selain itu lirik lagu. Editor langsung terbuka.
2. Tulis judul / referensi dan isi teks. **Satu baris kosong memisahkan slide**; hasilnya langsung terlihat di kolom **Hasil slide**. Perubahan tersimpan otomatis sekitar 1,5 detik setelah berhenti mengetik (Ctrl+S untuk menyimpan segera). Tekan **Kembali ke tayang** setelah selesai. Untuk mengubah materi yang ada, pilih materinya lalu **Edit materi**.
3. Tekan **+** di samping materi di Pustaka, atau **Tambah ke susunan** di editor. Materi yang sama dapat ditambahkan beberapa kali. Pilih baris di susunan untuk memunculkan tombol naik, turun, dan lepas.
4. Klik slide di tengah untuk pratinjau, lalu tekan **Tayangkan** atau **F5**. Klik dua kali slide, materi pustaka, atau baris susunan untuk langsung menayangkan. Pratinjau dan layar tayang terpisah, jadi persiapan tidak langsung mengubah layar jemaat.
5. Gunakan **Berikutnya / Sebelumnya** atau PageDown / PageUp. Setelah slide terakhir, Lentera berpindah ke materi berikutnya dalam susunan, melewati materi kosong.

Saat mengedit, layar tayang tidak berubah sampai Anda menayangkan lagi. Bilah atas menampilkan apa yang sedang tayang.

Dengan satu monitor, jendela proyektor terbuka sebagai jendela biasa. Dengan dua monitor, sambungkan proyektor **sebelum membuka aplikasi**, aktifkan mode **Extend / Perluas** di Windows, lalu pilih monitor proyektor di bilah atas; jendela proyektor terbuka layar penuh di monitor itu. Konfigurasi dua monitor perlu diuji pada perangkat ibadah yang sebenarnya sebelum digunakan.

## Tombol cepat

| Tombol | Fungsi |
|---|---|
| F5 / Ctrl+Enter | Tayangkan slide yang dipratinjau |
| F6 | Gelapkan / pulihkan layar tanpa menghilangkan posisi |
| F11 / klik ganda jendela proyektor | Beralih layar penuh |
| PageDown / PageUp | Slide berikutnya / sebelumnya |
| Panah kanan / kiri, Spasi | Navigasi saat tidak mengetik |
| Esc | Keluar layar penuh; tekan lagi untuk menyembunyikan proyektor |
| Ctrl+S | Simpan sekarang |
| Ctrl+N | Materi baru sesuai filter |

## Impor lirik dan ayat

Gunakan menu **Impor** di bilah atas.

**Lirik lagu (.txt):** berkas UTF-8; nama berkas menjadi judul. Pisahkan bagian lirik dengan baris kosong. Tersedia `contoh-lirik.txt`.

**Ayat Alkitab (.tsv):** satu ayat per baris, dengan format `referensi[TAB]teks`. `[TAB]` berarti karakter tab, bukan tulisan literal. Tersedia `contoh-ayat.tsv`. Jika perlu baris baru di dalam satu ayat, tulis `\n` pada kolom teks. Baris yang diawali `#` diabaikan. Duplikat referensi + isi dilewati. Seluruh berkas diperiksa sebelum data ditambahkan. Batas ukuran impor adalah 32 MB.

Pustaka menampilkan maksimal 500 hasil pertama agar tetap responsif. Ketik referensi atau kata di kolom pencarian untuk mempersempit hasil. Aplikasi ini tidak menyertakan terjemahan Alkitab lengkap atau database lagu berlisensi. Gunakan materi yang Anda berhak gunakan.

## Data dan cadangan

Data tersimpan di `%LOCALAPPDATA%\Lentera\library.json` (macOS: `~/Library/Application Support/Lentera`), termasuk susunan ibadah, pilihan monitor, dan latar. `library.json.bak` menyimpan versi sebelum penyimpanan terakhir. Cadangan ini bukan riwayat lengkap; salin folder data secara berkala, terutama sebelum impor besar. Aplikasi tidak mengganti data rusak dengan pustaka kosong. Hanya satu proses dapat membuka folder data yang sama.

Untuk menyimpan di lokasi lain, jalankan `lentera.exe --data-dir "D:\Data Ibadah"`. Setelah pembaruan lewat tombol **Perbarui**, Lentera dibuka kembali dengan folder data yang sama. Tutup aplikasi sebelum menyalin atau memulihkan cadangan. Layar tayang menyimpan salinan slide sehingga mengedit materi tidak langsung mengubah slide yang sedang ditampilkan.

## Ruang lingkup versi ini

- Lirik, ayat teks, pencarian, editor, susunan ibadah, pratinjau, keluaran terpisah, layar penuh, blackout, dan tiga warna latar.
- Windows 10/11 **64-bit** dengan installer dan pembaruan otomatis. macOS dari kode sumber. Antarmuka ditujukan untuk layar minimal sekitar 1024×640.
- Belum mencakup video, latar gambar, PowerPoint, NDI, streaming, pencarian rentang ayat otomatis, atau impor database EasyWorship.
- Belum ada sertifikat penandatanganan aplikasi. Jangan gunakan untuk layanan langsung sebelum latihan dan pengujian di komputer / proyektor tujuan.

## Untuk pengembang

Logika data, susunan ibadah, impor, dan penyimpanan ada di `crates/lentera-core` beserta pengujiannya. Aplikasi ada di `crates/lentera-app`: logika di `src/main.rs`, tata letak dan tema di `ui/app.slint`, pembaruan otomatis (Velopack) di `src/update.rs`. Tampilan digambar dengan renderer software Slint, tanpa browser tertanam atau server lokal. Tampilan Win32 lama ada di `crates/lentera-win32` dan tidak dirilis lagi. Rilis Windows dibuat oleh workflow `.github/workflows/rilis.yml` saat tag versi di-push.

Di Windows, pasang Rust stable dengan target `x86_64-pc-windows-msvc` serta Microsoft C++ Build Tools / Windows SDK. Di macOS, pasang Rust stable dan Xcode Command Line Tools. Dari folder proyek:

```powershell
cargo test --locked
cargo build -p lentera-app --release --locked
```

Konfigurasi `.cargo/config.toml` menautkan C runtime Windows secara statis. Jalankan perintah dari folder proyek agar Cargo membaca konfigurasi tersebut. Hasil build ada di `target/release/lentera.exe` (macOS: `target/release/lentera`).

Uji alur otomatis menggunakan direktori data percobaan terpisah:

```powershell
.\target\release\lentera.exe --data-dir ".\test-data" --ui-smoke-test
```

Hasilnya ditulis ke `test-data/ui-smoke-result.json`; proses menutup diri setelah pengujian. Untuk ikut menyimpan tangkapan layar PNG ke folder data, build dengan fitur `snapshot`:

```powershell
cargo run -p lentera-app --release --features snapshot -- --data-dir ".\test-data" --ui-smoke-test
```

Argumen `--update-source <folder atau URL>` mengganti sumber pembaruan, untuk menguji paket buatan lokal.

Kode sumber memakai lisensi MIT. Tampilan memakai Slint dengan lisensi Slint Royalty-free 2.0, yang mewajibkan atribusi di layar **Tentang**; jangan menghapus widget `AboutSlint` di `ui/app.slint`. Teks contoh adalah placeholder, bukan kumpulan lagu atau Alkitab berlisensi.
