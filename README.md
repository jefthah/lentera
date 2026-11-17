# Lentera

Presentasi ibadah ringan untuk Windows, dibangun dengan **Rust + Win32/GDI**. Fokus versi awal adalah lirik lagu, ayat teks, susunan ibadah, dan layar proyektor.

**Status: prototipe / beta 0.1.1.** Coba dahulu pada komputer dan proyektor tujuan sebelum dipakai dalam ibadah.

## Unduh untuk Windows

**[Unduh Lentera 0.1.1 — Installer Windows 64-bit](https://github.com/jefthah/lentera/releases/download/v0.1.1/LenteraApp-win-Setup.exe)**

[Halaman rilis dan berkas unduhan](https://github.com/jefthah/lentera/releases)

1. Jalankan installer di atas. Tidak perlu hak admin dan tidak perlu memasang Rust. Bila muncul "Windows protected your PC", pilih **More info → Run anyway**.
2. Buka **Lentera** dari Desktop atau Start Menu.
3. Pilih materi atau buat materi baru. Satu baris kosong memisahkan bagian / slide.
4. Tambahkan materi ke susunan, lalu tekan **F5** untuk menayangkan.

**Pembaruan otomatis.** Saat dibuka, Lentera memeriksa versi baru dan mengunduhnya di latar belakang. Pembaruan dipasang saat Lentera ditutup, atau segera lewat tombol **Perbarui ke …** di kanan bawah. Pembaruan tidak pernah dipasang selama Lentera berjalan.

Memerlukan **Windows 10/11 64-bit**. Antarmuka ditujukan untuk resolusi sekitar 1024×768 atau lebih pada skala 100%. Versi ini belum memiliki tanda tangan digital. Pengguna 0.1.0 cukup memasang installer ini sekali; data lama langsung terbaca.

## Fitur

- Pustaka lirik dan ayat, editor, serta pencarian judul / isi.
- Susunan ibadah dengan pengaturan urutan dan materi yang dapat dipakai berulang.
- Pratinjau terpisah dari materi yang sedang tayang.
- Jendela proyektor, pilihan monitor, layar penuh, dan blackout.
- Impor lirik TXT serta ayat TSV berformat UTF-8.
- Penyimpanan otomatis lokal dan cadangan berkas sebelumnya.
- Tiga warna latar dan ukuran teks yang menyesuaikan area tayang.
- Installer Windows dan pembaruan otomatis.

**F5**: tayangkan · **F6**: gelapkan / pulihkan · **F11**: layar penuh · **PageDown / PageUp**: navigasi saat tidak mengetik.

Untuk dua layar, hubungkan proyektor sebelum membuka Lentera, gunakan mode **Perluas / Extend** di Windows, lalu pilih monitor keluaran di aplikasi.

## Sasaran komputer ringan

Lentera menggunakan kontrol Windows native dan menggambar ulang saat ada perubahan, tanpa browser tertanam atau loop animasi. Sasaran pengembangannya adalah komputer RAM 2–4 GB.

Pengujian singkat versi 0.1.0 pada satu komputer pengembangan:

| Data | Peak working set proses |
|---|---:|
| Dua materi contoh | 12,78 MiB |
| Ditambah 31.000 ayat sintetis | 27,00 MiB |

Executable 0.1.0 berukuran 368.128 byte, sekitar 360 KiB. Sejak 0.1.1 menjadi sekitar 2,2 MB karena komponen pembaruan otomatis; peak working set pada data contoh tetap sekitar 13 MiB (diukur dengan skrip berbeda dari tabel di atas). Angka ini merupakan pengukuran awal, bukan jaminan penggunaan RAM atau kecepatan pada semua perangkat. Pengujian pada komputer RAM 2 GB belum dilakukan. Lihat [catatan pengujian](docs/PENGUJIAN.md).

## Data dan batasan beta

Data disimpan di `%LOCALAPPDATA%\Lentera\library.json`. Program terpasang terpisah di `%LOCALAPPDATA%\LenteraApp`, sehingga uninstall tidak menghapus data. Cadangan versi sebelum penyimpanan terakhir berada di `library.json.bak`. Hanya satu proses dapat membuka folder data yang sama. Tutup jendela Lentera yang sudah terbuka sebelum menjalankan salinan lain dengan data yang sama.

Pustaka menampilkan maksimal 500 hasil pertama. Gunakan pencarian untuk mempersempit materi. Belum tersedia video, latar gambar, PowerPoint, NDI, streaming, impor database EasyWorship, atau pengambilan rentang ayat otomatis.

Tidak menyertakan database Alkitab lengkap atau lagu berlisensi. Ayat dimasukkan manual atau diimpor dari TSV; gunakan materi yang Anda berhak gunakan. Teks contoh adalah placeholder.

## Dokumentasi dan pengembangan

- [Panduan penggunaan dan format impor](PANDUAN.md)
- [Riwayat rilis](CHANGELOG.md)
- [Catatan pengujian](docs/PENGUJIAN.md)

Repositori ini adalah workspace Cargo:

| Folder | Isi |
|---|---|
| `crates/lentera-core` | Data, susunan ibadah, impor, penyimpanan, dan kunci folder data. Tanpa kode tampilan; dibangun dan diuji di Windows dan macOS. |
| `crates/lentera-win32` | Tampilan Win32/GDI versi 0.1 (hanya Windows). Menghasilkan `lentera.exe`. |
| `prototypes/slint-spike` | Prototipe tampilan Slint untuk Windows dan macOS. Crate terpisah; lihat [hasilnya](prototypes/slint-spike/HASIL.md). |

Pasang Rust stable `x86_64-pc-windows-msvc`, Microsoft C++ Build Tools, dan Windows SDK. Dari folder repositori:

```powershell
cargo test --locked
cargo build --release --locked
```

Hasil build: `target/release/lentera.exe`. Konfigurasi proyek menautkan C runtime secara statis. Aplikasi bergantung pada DLL sistem Windows.

Di macOS, uji bagian inti saja: `cargo test -p lentera-core --locked`.

Rilis dibuat otomatis oleh workflow `rilis.yml` saat tag versi (misalnya `v0.1.1`) di-push: installer, ZIP portable, dan paket pembaruan diunggah sebagai draf pre-release. Pengguna baru menerima pembaruan setelah draf diterbitkan. Versi tag harus sama dengan `version` di `Cargo.toml`, dan `CHANGELOG.md` harus punya bagian untuk versi itu.

Kode sumber menggunakan [lisensi MIT](LICENSE). Lisensi dependensi tersedia di [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt); susun ulang dengan `python scripts/third_party_notices.py` setiap kali dependensi berubah.
