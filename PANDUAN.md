# Lentera 0.1 — presentasi ibadah ringan

Prototipe Windows native berbasis Rust, untuk lirik, ayat, dan proyektor pada komputer dengan RAM 2–4 GB. Target RAM merupakan sasaran pengembangan, bukan hasil uji pada semua komputer lama.

## Memasang dan memperbarui

1. Unduh **LenteraApp-win-Setup.exe** dari halaman rilis GitHub, lalu jalankan. Tidak perlu hak admin. Karena aplikasi belum bertanda tangan digital, Windows mungkin menampilkan "Windows protected your PC": pilih **More info → Run anyway**.
2. Buka Lentera dari shortcut **Lentera** di Desktop atau Start Menu.
3. Saat dibuka dan tersambung ke internet, Lentera memeriksa versi baru dan mengunduhnya di latar belakang. Bila sudah siap, muncul tombol **Perbarui ke …** di kanan bawah.
   - Biarkan saja: pembaruan dipasang otomatis saat Lentera ditutup.
   - Atau klik tombolnya, lalu pilih **Yes**: Lentera ditutup, diperbarui, dan dibuka kembali. Proyektor ikut tertutup sebentar, jadi lakukan di luar ibadah.
4. Untuk menghapus: Settings → Apps → **Lentera** → Uninstall. Data di `%LOCALAPPDATA%\Lentera` tidak ikut terhapus.

Pengguna 0.1.0 (ZIP lama) cukup memasang installer ini sekali; data lama langsung terbaca. Versi ZIP lama boleh dihapus setelahnya.

## Mulai memakai

1. Pilih materi di **Pustaka**. Pilih filter **Lirik lagu** atau **Ayat Alkitab**, lalu **Baru** untuk membuat materi sesuai jenisnya.
2. Tulis judul / referensi dan isi teks di editor. **Satu baris kosong memisahkan slide.** Perubahan disimpan otomatis setelah berhenti mengetik sekitar 1,5 detik; tombol **Simpan** juga tersedia.
3. Klik **Tambah ke susunan**. Atur urutan melalui **Naik**, **Turun**, dan **Lepas**. Materi yang sama dapat ditambahkan beberapa kali.
4. Klik bagian yang ingin ditampilkan, periksa **Pratinjau**, lalu klik **Tayangkan** atau tekan **F5**. Pratinjau dan layar yang sedang tayang terpisah agar persiapan materi tidak langsung mengubah layar jemaat.
5. Gunakan **Berikutnya / Sebelumnya**. Setelah bagian terakhir, aplikasi berpindah ke materi berikutnya dalam susunan, melewati materi kosong.

Dengan satu monitor, keluaran terbuka sebagai jendela biasa. Dengan dua monitor, sambungkan proyektor **sebelum membuka aplikasi**, aktifkan mode **Extend / Perluas** di Windows, lalu pilih monitor keluaran di Lentera. Jendela keluaran mendukung layar penuh. Konfigurasi dua monitor perlu diuji pada perangkat ibadah yang sebenarnya sebelum digunakan.

## Tombol cepat

| Tombol | Fungsi |
|---|---|
| F5 / Ctrl+Enter | Tayangkan bagian terpilih |
| F6 | Gelapkan / pulihkan layar tanpa menghilangkan posisi |
| F11 / klik ganda jendela proyektor | Beralih layar penuh |
| PageDown / PageUp | Bagian berikutnya / sebelumnya saat tidak mengetik |
| Panah kanan / kiri, Spasi | Navigasi saat tidak mengetik |
| Esc | Keluar layar penuh; tekan lagi untuk menyembunyikan proyektor |
| Ctrl+S | Simpan |
| Ctrl+N | Materi baru sesuai filter |

## Impor lirik dan ayat

**Impor lirik .txt:** berkas UTF-8; nama berkas menjadi judul. Pisahkan bagian lirik dengan baris kosong. Tersedia `contoh-lirik.txt`.

**Impor ayat .tsv:** satu ayat per baris, dengan format `referensi[TAB]teks`. `[TAB]` berarti karakter tab, bukan tulisan literal. Tersedia `contoh-ayat.tsv`. Jika perlu baris baru di dalam satu ayat, tulis `\n` pada kolom teks. Baris yang diawali `#` diabaikan. Duplikat referensi + isi dilewati. Seluruh berkas diperiksa sebelum data ditambahkan. Batas ukuran impor adalah 32 MB.

Pustaka menampilkan maksimal 500 hasil pertama agar tetap responsif. Ketik referensi atau kata di kolom pencarian untuk mempersempit hasil. Aplikasi ini tidak menyertakan terjemahan Alkitab lengkap atau database lagu berlisensi. Gunakan materi yang Anda berhak gunakan.

## Data dan cadangan

Data tersimpan di `%LOCALAPPDATA%\Lentera\library.json`, termasuk susunan ibadah dan pilihan latar. `library.json.bak` menyimpan versi sebelum penyimpanan terakhir. Cadangan ini bukan riwayat lengkap; salin folder data secara berkala, terutama sebelum impor besar. Aplikasi tidak mengganti data rusak dengan pustaka kosong. Hanya satu proses dapat membuka folder data yang sama.

Untuk menyimpan di lokasi lain, jalankan `Lentera.exe --data-dir "D:\Data Ibadah"`. Setelah pembaruan lewat tombol **Perbarui**, Lentera dibuka kembali dengan folder data yang sama. Tutup aplikasi sebelum menyalin atau memulihkan cadangan. Layar tayang menyimpan salinan slide sehingga mengedit materi tidak langsung mengubah slide yang sedang ditampilkan.

## Ruang lingkup versi ini

- Lirik, ayat teks, pencarian, editor, susunan ibadah, pratinjau, keluaran terpisah, layar penuh, blackout, dan tiga warna latar.
- Windows 10/11 **64-bit**. Antarmuka ditujukan untuk layar minimal sekitar 1024×768 pada skala 100%.
- Belum mencakup video, latar gambar, PowerPoint, NDI, streaming, pencarian rentang ayat otomatis, atau impor database EasyWorship.
- Sudah ada installer dan pembaruan otomatis, tetapi belum ada sertifikat penandatanganan aplikasi. Jangan gunakan untuk layanan langsung sebelum latihan dan pengujian di komputer / proyektor tujuan.

## Untuk pengembang

Logika data, susunan ibadah, impor, dan penyimpanan ada di `crates/lentera-core` beserta pengujiannya. Tampilan Windows ada di `crates/lentera-win32/src/main.rs`; pembaruan otomatis (Velopack) di `crates/lentera-win32/src/update.rs`. Rilis dibuat oleh workflow `.github/workflows/rilis.yml` saat tag versi di-push. UI menggunakan Win32 + GDI, tanpa browser tertanam, loop animasi, atau server lokal. Aplikasi menunggu pesan Windows dan menggambar saat ada perubahan. Pencarian dibatasi 500 hasil tampilan.

Pasang Rust stable dengan target `x86_64-pc-windows-msvc` serta Microsoft C++ Build Tools / Windows SDK. Dari folder proyek:

```powershell
cargo test --locked
cargo build --release --locked
```

Konfigurasi `.cargo/config.toml` menautkan C runtime secara statis. Jalankan perintah dari folder proyek agar Cargo membaca konfigurasi tersebut. Hasil build ada di `target/release/lentera.exe`.

Uji jendela otomatis menggunakan direktori data percobaan terpisah:

```powershell
.\target\release\lentera.exe --data-dir ".\test-data" --ui-smoke-test
```

Hasilnya ditulis ke `test-data/ui-smoke-result.json`; proses menutup diri setelah pengujian. Kode sumber memakai lisensi MIT. Teks contoh adalah placeholder, bukan kumpulan lagu atau Alkitab berlisensi.
