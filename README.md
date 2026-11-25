# Lentera

<img src="crates/lentera-app/assets/icon/lentera.svg" alt="Logo Lentera" width="96">

Presentasi ibadah ringan untuk Windows dan macOS, dibangun dengan **Rust + Slint**. Fokus versi awal adalah lirik lagu, ayat Alkitab, susunan ibadah, dan layar proyektor.

**Status: beta 0.8.0.** Coba dahulu pada komputer dan proyektor tujuan sebelum dipakai dalam ibadah.

<a href="https://slint.dev"><img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" alt="#MadeWithSlint" height="60"></a>

## Unduh untuk Windows

**[Unduh Lentera 0.8.0 — Installer Windows 64-bit](https://github.com/jefthah/lentera/releases/download/v0.8.0/LenteraApp-win-Setup.exe)**

[Halaman rilis dan berkas unduhan](https://github.com/jefthah/lentera/releases)

1. Jalankan installer di atas. Tidak perlu hak admin dan tidak perlu memasang Rust. Bila muncul "Windows protected your PC", pilih **More info → Run anyway**.
2. Buka **Lentera** dari Desktop atau Start Menu, lalu masuk dengan email dan kode 6 digit yang dikirim ke email itu.
3. Pilih materi di **Pustaka** atau tekan **Baru**. Satu baris kosong memisahkan slide.
4. Tekan **+ Susunan** untuk menambahkan materi ke susunan ibadah, pilih slide, lalu tekan **Tayangkan** atau **F5**.

**Pembaruan otomatis.** Saat dibuka, Lentera memeriksa versi baru dan mengunduhnya di latar belakang. Pembaruan dipasang saat Lentera ditutup, atau segera lewat tombol **Perbarui ke …** di kanan bawah. Pembaruan tidak pernah dipasang selama Lentera berjalan. Pengguna 0.1.1 sampai 0.7.0 menerima 0.8.0 dengan cara ini; pengguna 0.1.0 perlu memasang installer di atas sekali. Bila memperbarui dari 0.6.0 atau sebelumnya, Lentera meminta masuk dengan email saat pertama kali dibuka. Setelah 0.5.0 atau yang lebih baru membuka sebuah profil, profil itu tidak bisa dibuka lagi oleh 0.4.0 atau sebelumnya. Bila memperbarui dari 0.2.1 atau sebelumnya, pustaka lama dipindahkan otomatis ke **Profil utama** saat versi baru pertama kali dibuka.

Memerlukan **Windows 10/11 64-bit**. Antarmuka ditujukan untuk layar sekitar 1024×640 atau lebih. Versi ini belum memiliki tanda tangan digital.

**macOS:** belum ada installer. Aplikasi sudah dibangun dan diuji otomatis di macOS lewat GitHub Actions, dan dapat dibangun dari kode sumber (lihat bagian pengembangan).

## Tampilan dan fitur

- **Kiri:** susunan ibadah (naik, turun, lepas), lalu tab **Pustaka** (lirik / ayat dengan filter dan pencarian judul / isi) dan tab **Alkitab**.
- **Alkitab:** impor terjemahan sekali (OpenSong XML, Zefania XML, TSV, atau EasyWorship `.ewb`). Tab **Alkitab** membuka Alkitab di kolom tengah: ketik referensi seperti `Yoh 3:16-18` dan tekan Enter untuk menayangkan, atau ketik referensinya di kotak cari (Ctrl+K). Ayat juga bisa dipilih dari daftar satu pasal (Shift+klik untuk rentang), dan **Terakhir dibuka** di kiri membuka lagi referensi sebelumnya. Slide dibagi otomatis 1–3 ayat dengan nomor ayat kecil, dan proyektor menulis referensi ayat di bawahnya; **Ke susunan** menyimpannya ke pustaka dan susunan ibadah.
- **Tengah:** semua slide materi terpilih sebagai kartu. Klik untuk pratinjau, klik dua kali untuk langsung menayangkan.
- **Kanan:** pratinjau, tombol **Tayangkan**, layar yang sedang tayang, tombol sebelumnya / berikutnya, gelapkan, layar penuh, dan latar umum (tiga warna, gambar, atau video).
- **Latar gambar dan video:** tambahkan gambar JPG atau PNG, atau video MP4, MOV, atau WMV, ke galeri profil. Pakai satu sebagai latar umum, atau beri setiap lagu dan ayat latarnya sendiri. Teks di atasnya tampil putih dengan garis tepi gelap. Gambar disalin ke profil dan diperkecil untuk layar 1920×1080. Berkas aslinya tidak diubah.
  - Video diputar berulang tanpa suara di proyektor dan didecode di GPU lewat Media Foundation bawaan Windows. Video latar belum tersedia di macOS.
- **Profil:** beberapa profil dalam satu komputer, masing-masing dengan pustaka dan susunan ibadahnya sendiri. Buat, ganti nama, hapus, dan berganti profil lewat tombol profil di bilah atas.
- **Akun dan sinkron:** masuk dengan kode email tanpa kata sandi, lalu pakai profil yang sama di beberapa komputer. Lirik, ayat, susunan ibadah, nama profil, dan warna latar umum ikut disinkron; Alkitab, gambar dan video latar, serta pilihan monitor tetap per komputer. Setelah masuk sekali, Lentera tetap bisa dipakai tanpa internet.
- **Editor materi:** judul, jenis (lirik / ayat), dan isi, dengan hasil slide yang langsung terlihat. Tersimpan otomatis.
- Jendela proyektor terpisah dengan pilihan monitor, layar penuh, dan blackout. Ukuran teks menyesuaikan layar.
- Impor lirik TXT dan ayat TSV berformat UTF-8, serta terjemahan Alkitab dari OpenSong XML, Zefania XML, atau TSV.
- **Pindah dari EasyWorship:** semua lagu EasyWorship 2009, 6, 7, atau 8 diimpor sekaligus langsung dari databasenya, tanpa lagu ganda; Alkitab `.ewb` juga bisa diimpor. Berkas EasyWorship hanya dibaca.
- Penyimpanan otomatis lokal dan cadangan berkas sebelumnya.
- Installer Windows dan pembaruan otomatis.

**F5**: tayangkan · **F6**: gelapkan / pulihkan · **F11**: layar penuh · **PageDown / PageUp**: navigasi saat tidak mengetik.

Untuk dua layar, hubungkan proyektor sebelum membuka Lentera, gunakan mode **Perluas / Extend** di Windows, lalu pilih monitor proyektor lewat panah di tombol proyektor di bilah atas.

## Sasaran komputer ringan

Lentera tidak memakai browser tertanam. Tampilan digambar dengan renderer software Slint (tanpa OpenGL), sehingga tidak bergantung pada driver kartu grafis. Sasaran pengembangannya adalah komputer RAM 2–4 GB.

Pengukuran versi 0.2.0 pada satu komputer pengembangan (peak working set):

| Data | 0.2.0 (Slint) | 0.1.1 (Win32) |
|---|---:|---:|
| Dua materi contoh | 34 MiB | 15 MiB |
| Ditambah 31.000 ayat sintetis | 42 MiB | 29 MiB |

Program 0.2.0 berukuran sekitar 11 MB (0.1.1: 2,2 MB). Angka ini merupakan pengukuran awal, bukan jaminan penggunaan RAM atau kecepatan pada semua perangkat. Pengujian pada komputer RAM 2 GB belum dilakukan. Lihat [catatan pengujian](docs/PENGUJIAN.md).

## Data dan batasan beta

Data disimpan per profil dalam basis data SQLite di `%LOCALAPPDATA%\Lentera\profiles\<id profil>\library.db` (macOS: `~/Library/Application Support/Lentera`). Profil pertama bernama **Profil utama**; profil lain dibuat lewat layar **Pilih profil**, dan profil yang dihapus dipindahkan ke folder `profiles-deleted`. Isi `library.json` dari versi 0.2.1 atau sebelumnya dipindahkan otomatis saat pertama kali dibuka, dan berkas lamanya dibiarkan sebagai cadangan. Terjemahan Alkitab yang diimpor disimpan di `alkitab.db` di folder data, satu untuk semua profil. Program terpasang terpisah di `%LOCALAPPDATA%\LenteraApp`, sehingga uninstall tidak menghapus data. Cadangan data saat Lentera terakhir dibuka berada di `library.db.bak`. Dengan akun, salinan materi, susunan ibadah, dan nama profil juga disimpan di server Lentera; token masuk disimpan di Windows Credential Manager, bukan di folder data. Hanya satu proses dapat membuka folder data yang sama. Tutup jendela Lentera yang sudah terbuka sebelum menjalankan salinan lain dengan data yang sama.

Pustaka menampilkan semua materi (diuji sampai 5.000 lagu). Gunakan pencarian untuk mempersempit materi. Belum tersedia video sebagai materi tayang (dengan suara), PowerPoint, NDI, streaming, atau impor jadwal dan media EasyWorship.

Tidak menyertakan teks Alkitab atau lagu berlisensi. Terjemahan Alkitab diimpor dari berkas OpenSong XML, Zefania XML, TSV, atau EasyWorship `.ewb`; gunakan materi yang Anda berhak gunakan. Teks contoh adalah placeholder.

## Dokumentasi dan pengembangan

- [Panduan penggunaan dan format impor](PANDUAN.md)
- [Riwayat rilis](CHANGELOG.md)
- [Catatan pengujian](docs/PENGUJIAN.md)
- [Protokol login dan sinkron](docs/SINKRON.md) (untuk pengembang)

Repositori ini adalah workspace Cargo:

| Folder | Isi |
|---|---|
| `crates/lentera-core` | Data, susunan ibadah, impor, penyimpanan, dan kunci folder data. Tanpa kode tampilan; dibangun dan diuji di Windows dan macOS. |
| `crates/lentera-app` | Aplikasi Lentera dengan tampilan Slint untuk Windows dan macOS. Menghasilkan `lentera.exe` / `lentera`, yang dirilis sejak 0.2.0. |
| `crates/lentera-win32` | Tampilan Win32/GDI versi 0.1 (hanya Windows). Tidak dirilis lagi; menghasilkan `lentera-win32.exe`. |
| `server` | Server login dan sinkron (Go + PostgreSQL). Modul Go terpisah dan sudah di-deploy. Build rilis mengaktifkan layar masuk setelah variabel repositori `LENTERA_SERVER` diisi. Lihat [README-nya](server/README.md). |
| `prototypes/slint-spike` | Prototipe tampilan Slint. Crate terpisah; lihat [hasilnya](prototypes/slint-spike/HASIL.md). |

Di Windows, pasang Rust stable `x86_64-pc-windows-msvc`, Microsoft C++ Build Tools, dan Windows SDK. Di macOS, pasang Rust stable dan Xcode Command Line Tools. Dari folder repositori:

```powershell
cargo test --locked
cargo build -p lentera-app --release --locked
```

Hasil build: `target/release/lentera.exe` (macOS: `target/release/lentera`). Di Windows, C runtime ditautkan statis dan aplikasi hanya bergantung pada DLL sistem. Untuk mencoba dengan data percobaan terpisah:

```powershell
cargo run -p lentera-app --release -- --data-dir ".\test-data"
```

Rilis Windows dibuat otomatis oleh workflow `rilis.yml` saat tag versi (misalnya `v0.2.0`) di-push: installer, ZIP portable, dan paket pembaruan diunggah sebagai draf pre-release. Pengguna baru menerima pembaruan setelah draf diterbitkan. Versi tag harus sama dengan `version` di `Cargo.toml`, `CHANGELOG.md` harus punya bagian untuk versi itu, dan variabel repositori `LENTERA_SERVER` harus berisi alamat server.

Kode sumber menggunakan [lisensi MIT](LICENSE). Tampilan memakai [Slint](https://slint.dev) dengan lisensi Slint Royalty-free 2.0; atribusinya tampil di layar **Tentang** (tombol ⓘ di bilah atas) dan pada badge di atas. Lisensi dependensi tersedia di [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt); susun ulang dengan `python scripts/third_party_notices.py` setiap kali dependensi berubah.
