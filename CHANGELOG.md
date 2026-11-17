# Riwayat rilis

## 0.1.1 — 17 November 2026 — beta

Installer dan pembaruan otomatis untuk Windows 10/11 64-bit.

- **Installer** `LenteraApp-win-Setup.exe`: memasang untuk pengguna saat ini tanpa hak admin, membuat shortcut di Desktop dan Start Menu, dan bisa di-uninstall dari Settings → Apps. Tersedia juga `LenteraApp-win-Portable.zip`.
- **Pembaruan otomatis:** saat dibuka, Lentera memeriksa versi baru di GitHub dan mengunduhnya di latar belakang. Pembaruan dipasang saat Lentera ditutup, atau segera lewat tombol **Perbarui ke …** setelah konfirmasi. Pembaruan tidak pernah dipasang selama Lentera berjalan. Tanpa internet, pemeriksaan dilewati tanpa pesan.
- Program terpasang di `%LOCALAPPDATA%\LenteraApp`. Data tetap di `%LOCALAPPDATA%\Lentera`, dan uninstall tidak menghapus data.
- Nomor versi tampil di judul jendela.
- Pengguna 0.1.0 perlu memasang 0.1.1 sekali secara manual. Versi berikutnya datang otomatis.
- Program menjadi sekitar 2,2 MB (sebelumnya 0,36 MB) karena komponen unduh dan pasang pembaruan.
- Kode dipecah menjadi `lentera-core` (data, tanpa tampilan, berjalan di Windows dan macOS) dan `lentera-win32` (tampilan Windows). Ini persiapan tampilan Slint untuk Windows dan macOS.
- Folder data bawaan di macOS: `~/Library/Application Support/Lentera`. Windows tetap `%LOCALAPPDATA%\Lentera`.
- Kunci folder data memakai mekanisme kunci berkas yang juga tersedia di macOS. Pesan galat kini menyebut bila jendela Lentera lain sedang membuka data yang sama.
- Fitur dan format `library.json` tidak berubah.

## 0.1.0 — 16 November 2026 — beta

Rilis prototipe pertama untuk Windows 10/11 64-bit.

- Editor dan pustaka lirik / ayat dengan pencarian.
- Susunan ibadah, pembagian slide berdasarkan baris kosong, dan navigasi antarbagian / materi.
- Pratinjau, keluaran proyektor terpisah, pilihan monitor, layar penuh, serta blackout.
- Impor TXT/TSV UTF-8 dengan pemeriksaan format dan penanganan duplikat ayat.
- Penyimpanan lokal otomatis, cadangan, dan penguncian folder data.
- UI native Rust + Win32/GDI dan C runtime statis.

Belum mencakup video, gambar, PowerPoint, NDI, streaming, database Alkitab lengkap, installer, tanda tangan digital, pembaruan otomatis, atau impor database EasyWorship.
