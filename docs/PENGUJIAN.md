# Pengujian Lentera

Semua pengujian dilakukan di satu komputer pengembangan Windows x64, kecuali hasil macOS dari GitHub Actions. Bagian awal mencatat versi 0.1.0 (16 November 2026); bagian berikutnya urut per perubahan. Hasil ini tidak mewakili semua komputer lama.

## Verifikasi kode

- `cargo test --locked`: 7 pengujian lulus.
- `cargo clippy --all-targets --locked -- -D warnings`: tanpa peringatan.
- Build release berhasil, ukuran executable 368.128 byte.
- Dependensi executable hanya DLL sistem Windows; C runtime ditautkan statis.

Pengujian model memeriksa pemisahan slide dan Unicode, impor yang gagal tanpa perubahan parsial, duplikat ayat, referensi susunan setelah penghapusan, pencarian, penulisan / pembacaan data dan cadangan, serta penolakan data rusak / versi tidak didukung.

## Uji alur aplikasi

Sembilan pemeriksaan otomatis dijalankan pada dua dataset, semuanya lulus:

1. Menayangkan slide pertama.
2. Memindahkan ke slide berikutnya.
3. Memindahkan ke materi berikutnya dalam susunan.
4. Kembali ke materi sebelumnya.
5. Mengaktifkan blackout.
6. Beralih ke layar penuh.
7. Kembali ke mode jendela.
8. Mencari berdasarkan isi teks.
9. Mengedit judul Unicode dan memverifikasi hasil penyimpanan.

Windows melaporkan dua monitor saat pengujian. Keberhasilan pemeriksaan jendela otomatis belum membuktikan bahwa semua konfigurasi proyektor bekerja dengan benar.

## Pemakaian memori awal

Proses disampel setiap sekitar 100 ms sepanjang pengujian singkat sekitar 3,6 detik. Angka berikut adalah nilai puncak yang teramati selama sampling, bukan pengukuran jangka panjang.

| Dataset | Working set | Private memory |
|---|---:|---:|
| Dua materi contoh | 12,78 MiB | 2,03 MiB |
| Dua materi + 31.000 ayat sintetis | 27,00 MiB | 26,77 MiB |

Dataset besar memakai teks sintetis, bukan terjemahan Alkitab berlisensi.

## Yang masih perlu diuji

- Pemeriksaan visual manual menyeluruh.
- Komputer sasaran dengan RAM 2–4 GB dan perangkat keras lama.
- Pemakaian ibadah dalam durasi panjang.
- Resolusi, skala tampilan, dan proyektor tujuan.

Untuk mengulang uji alur setelah build, gunakan folder data percobaan baru yang terpisah:

```powershell
.\target\release\lentera.exe --data-dir ".\test-data" --ui-smoke-test
```

Hasil ditulis ke `test-data/ui-smoke-result.json`. Jangan gunakan folder data ibadah nyata untuk pengujian ini.

## Pemecahan core + app — 17 November 2026

Kode dipecah menjadi `crates/lentera-core` dan `crates/lentera-win32`. Diuji di komputer pengembangan yang sama (Windows 11, Rust 1.100.0):

- `cargo test --locked`: 16 pengujian lulus (7 lama + 9 baru untuk navigasi susunan, slide, editor, impor berkas, folder data per OS, dan kunci folder data).
- `cargo clippy --all-targets --locked -- -D warnings` dan `cargo fmt --all --check`: bersih.
- `cargo clippy -p lentera-core --target aarch64-apple-darwin`: bersih. Ini hanya memastikan inti dapat dikompilasi untuk macOS; tes di Mac dijalankan lewat GitHub Actions.
- Executable 369.152 byte (sebelumnya 368.128).
- Uji alur UI: 9 dari 9 pemeriksaan lulus.
- Kunci folder data: saat aplikasi berjalan, proses lain tidak bisa mengunci `library.lock`; setelah aplikasi ditutup, kunci lepas.
- Peak working set pada data contoh, diukur dengan skrip yang sama untuk kedua versi: 0.1.0 = 14,76 MiB, hasil pemecahan = 14,79 MiB. Angka ini memakai metode berbeda dari tabel di atas, jadi bandingkan hanya satu sama lain.

## Installer dan pembaruan otomatis 0.1.1 — 17 November 2026

Diuji di komputer pengembangan yang sama dengan paket buatan lokal (`vpk` 1.2.161, crate `velopack` 1.2.161). Sumber pembaruan diarahkan ke folder lokal lewat `--update-source`; jalur GitHub Releases baru teruji setelah rilis pertama diterbitkan.

- `cargo test --locked` 16 lulus; clippy bersih; uji alur UI 9 dari 9 lulus; peak working set 13,19 MiB.
- Executable 2.236.416 byte. Installer `LenteraApp-win-Setup.exe` sekitar 10,7 MB; paket pembaruan penuh 3,2 MB; paket delta untuk perubahan kecil 4 KB.
- **Pasang** (`Setup.exe --silent`): terpasang di `%LOCALAPPDATA%\LenteraApp`, shortcut Desktop dan Start Menu dibuat, entri uninstall "Lentera" ada.
- **Pembaruan saat ditutup** (0.1.1 → 0.1.2): versi baru terunduh di latar belakang, tombol **Perbarui ke 0.1.2** dan pesan status muncul. Setelah jendela ditutup, 0.1.2 terpasang tanpa membuka ulang Lentera.
- **Pembaruan lewat tombol** (0.1.3 → 0.1.4): dialog konfirmasi menunggu (dibiarkan 20 detik, tidak ada yang terpasang). **No**: tidak ada perubahan, tombol tetap ada. **Yes**: Lentera ditutup, diperbarui, dan dibuka kembali dengan `--data-dir` yang sama.
- Versi terbaru tidak menampilkan tombol pembaruan.
- **Uninstall** (`Update.exe uninstall`): folder program, shortcut, dan entri uninstall terhapus. Folder data uji dan `%LOCALAPPDATA%\Lentera` tetap utuh (hash sama dengan cadangan sebelum uji).

Catatan: `Setup.exe` dari Velopack 1.2.161 crash bila diberi argumen tambahan setelah `--`. Lentera tidak memakai fitur itu. Versi portable dan pembaruan dari GitHub Releases belum diuji di sini.

## Tampilan baru Slint (`lentera-app`) — 17 November 2026

Belum dirilis. Diuji di komputer pengembangan yang sama (Windows 11, dua monitor, Slint 1.19.0, renderer software).

- `cargo clippy -p lentera-app --all-targets --locked -- -D warnings` (dengan dan tanpa fitur `snapshot`) dan `cargo fmt --all --check`: bersih.
- Uji alur UI: 11 dari 11 pemeriksaan lulus, yaitu 9 pemeriksaan yang sama dengan versi Win32 ditambah pembagian slide di editor dan kembali dari editor. Lulus juga pada data dengan 31.000 ayat sintetis.
- Tangkapan layar jendela operator (mode tayang dan edit) serta jendela proyektor diperiksa mata dan sesuai rancangan.
- Executable rilis 11.051.008 byte (Win32: 2.234.880 byte).
- Peak working set, diukur dengan skrip yang sama untuk kedua tampilan:

| Data | Slint (`lentera-app`) | Win32 (`lentera.exe`) |
|---|---:|---:|
| Contoh, dibuka tanpa tindakan | 34,1 MiB (private 8,5) | 15,1 MiB (private 2,2) |
| Contoh, uji alur | 34,4 MiB (private 16,7) | 15,3 MiB (private 2,3) |
| Contoh + 31.000 ayat sintetis, uji alur | 42,1 MiB (private 25,9) | 29,1 MiB (private 16,7) |

- macOS: `cargo check --target aarch64-apple-darwin` tidak bisa dijalankan dari Windows karena crate `ring` (dipakai Velopack untuk HTTPS) perlu compiler C untuk macOS. Build, clippy, dan uji alur macOS dijalankan di GitHub Actions (job **Aplikasi Slint** di `inti.yml`).

Belum diuji: pemakaian keyboard dan mouse secara manual, macOS di Mac sungguhan, proyektor sungguhan, skala tampilan selain 100%, dan pembaruan otomatis lewat installer untuk aplikasi ini.

## Rilis 0.2.0 dan pembaruan dari 0.1.1 — 17 November 2026

Sejak 0.2.0, `lentera.exe` yang dirilis adalah aplikasi Slint (`crates/lentera-app`). Tampilan Win32 kini bernama `lentera-win32.exe` dan tidak dirilis. Diuji di komputer pengembangan yang sama.

- `cargo test --locked` 16 lulus; `cargo clippy --all-targets` (juga dengan fitur `snapshot`) dan `cargo fmt --all --check` bersih.
- Uji alur UI: Slint 11 dari 11, Win32 (`lentera-win32.exe`) 9 dari 9.
- Layar **Tentang** dengan widget `AboutSlint` tampil (tangkapan layar diperiksa mata). Manifest Windows (DPI per monitor, dialog bergaya modern) tertanam di `lentera.exe`.
- Executable rilis 11.117.056 byte.
- THIRD-PARTY-NOTICES disusun ulang dari crate yang benar-benar dibangun (`cargo tree`): 294 crate, semuanya dengan teks lisensi. Crate Slint dicantumkan dengan lisensi Royalty-free 2.0.
- CI commit `52b2b46` (run 44107733899): build, clippy, dan uji alur UI aplikasi Slint lulus di Windows dan **macOS** (runner `macos-latest`). Sejak itu uji alur macOS menjadi wajib di CI. Tangkapan layar macOS belum diperiksa.
- Commit `ddcd2ec` lulus uji alur macOS pada run push (44112713498) tetapi gagal pada run pull request (44113014516), dengan kode yang sama. Pemeriksaan yang terlihat di log lulus; dugaan penyebabnya pemeriksaan layar penuh / jendela yang dilakukan 0,45 detik setelah perpindahan, sedangkan macOS memakai animasi. Kedua pemeriksaan itu kini menunggu hingga sekitar 5 detik sampai keadaan jendela berubah, dan laporan uji mencantumkan daftar `failed`. Di Windows uji alur lulus tiga kali berturut-turut setelah perubahan ini.

**Pembaruan.** Diuji dengan paket uji terpisah (`--packId LenteraAppUji`, tanpa shortcut) dalam bentuk ZIP portable, supaya Lentera yang terpasang di komputer ini tidak tersentuh. Versi 0.1.1 memakai salinan `lentera.exe` 0.1.1 yang terpasang; 0.2.0–0.2.2 memakai build rilis aplikasi Slint. `vpk` dan crate `velopack` 1.2.161; sumber pembaruan lewat `--update-source`.

- **0.1.1 (Win32) → 0.2.0 (Slint), saat ditutup:** 0.1.1 mengunduh 0.2.0 di latar belakang. Setelah jendela ditutup, 0.2.0 terpasang tanpa membuka ulang. `library.json` byte-identik sebelum dan sesudah. Aplikasi hasil pembaruan lulus uji alur 11 dari 11 dengan versi 0.2.0.
- **0.2.0 → 0.2.1, saat ditutup:** status "Lentera 0.2.1 sudah diunduh …" dan tombol **Perbarui ke 0.2.1** muncul (dibaca lewat UI Automation). Paket delta 2 KB. Setelah ditutup, 0.2.1 terpasang; data tetap byte-identik.
- **0.2.1 → 0.2.2, lewat tombol:** dialog "Perbarui Lentera" muncul dan menunggu (dibiarkan 3 detik, tidak ada yang terpasang). **Yes**: Lentera ditutup, diperbarui ke 0.2.2, lalu dibuka kembali dengan `--data-dir` dan `--update-source` yang sama. Dialog dicari dengan Win32 FindWindow karena UI Automation tidak melihatnya.
- Ukuran paket uji: 0.2.0 penuh 8,1 MB, delta 0.1.1 → 0.2.0 5,1 MB, ZIP portable 8,1 MB, `Setup.exe` 15,6 MB.

Belum diuji: `Setup.exe` 0.2.0 (hanya versi portable), pembaruan dari GitHub Releases, SmartScreen, Mac sungguhan, proyektor sungguhan, dan pemakaian keyboard / mouse secara manual.

## Rilis 0.2.1: logo dan ikon aplikasi — 18 November 2026

Diuji di komputer pengembangan yang sama (Windows 11, dua monitor, skala 100%).

- `cargo test --locked` 16 lulus; `cargo clippy -p lentera-app --all-targets` (juga dengan fitur `snapshot`) dan `cargo fmt --all --check` bersih. CI commit `7eac602` lulus di Windows dan macOS (run 44327335824 dan 44327581408).
- `lentera.ico` berisi 16, 20, 24, 32, 40, 48, 64 (bitmap) dan 256 px (PNG). Ukuran 16–32 px digambar dari `lentera-kecil.svg`.
- `lentera.exe`: ikon terbaca pada 16, 32, dan 256 px lewat `PrivateExtractIcons`; info versi FileDescription/ProductName "Lentera", FileVersion/ProductVersion 0.2.1.
- Jendela operator yang berjalan memakai ikon 16×16 untuk title bar dan 32×32 untuk taskbar (dibaca dengan `WM_GETICON`). Uji alur UI lulus; tangkapan layar menunjukkan logo di bilah atas dan layar **Tentang**.
- `vpk pack` 1.2.161 dengan `--icon` (paket uji `LenteraAppUji`, tidak dipasang): `Setup.exe`, `Update.exe`, dan peluncur `Lentera.exe` di paket memakai ikon Lentera; peluncur juga membawa info versi 0.2.1.
- Shortcut Desktop dan Start Menu yang dibuat installer menunjuk ke `LenteraApp\current\lentera.exe`, jadi ikonnya ikut berganti setelah pembaruan terpasang (bisa tertunda oleh cache ikon Windows).

Belum diuji: pembaruan 0.2.0 → 0.2.1 dari GitHub Releases, tampilan ikon pada skala tampilan selain 100%, dan Settings → Apps.

## Penyimpanan SQLite per profil — 19 November 2026

Belum dirilis. Diuji di komputer pengembangan yang sama (Windows 11, dua monitor), build rilis tanpa fitur `snapshot`.

- `cargo test -p lentera-core --locked`: 22 lulus, termasuk 6 tes baru untuk penyimpanan: profil pertama berisi materi contoh; `library.json` dipindahkan tanpa diubah dan tidak dipindahkan dua kali; `library.json` rusak tidak membuat profil; hanya materi yang berubah yang ditulis, materi terhapus menjadi tanda hapus, id lokal tidak dipakai ulang, dan cadangan berisi keadaan saat dibuka; profil yang pembuatannya terputus diabaikan dan profil dari versi lebih baru menolak dibuka; pustaka tidak valid tidak disimpan. `cargo clippy --workspace --all-targets` (juga `lentera-app` dengan `snapshot`) dan `cargo fmt --all --check` bersih.
- Uji alur UI di folder kosong: `lentera` 11/11, `lentera-win32` 9/9. Satu profil terbentuk dengan materi contoh.
- Pemindahan `library.json` 5,1 MB (2 materi contoh + 31.000 ayat sintetis): uji alur `lentera` 11/11 dan `lentera-win32` 9/9. Isi basis data dibandingkan dengan JSON: 31.002 materi sama persis (kecuali materi 1 yang memang diubah uji alur), susunan, latar, dan monitor sama, setiap materi punya UUID v7 unik, dan `library.json` tidak berubah. `library.db` 5,8 MB; `library.db.bak` terbentuk saat dibuka ulang.
- Salinan `library.json` asli pengguna (3 materi) dibuka biasa lalu ditutup: ketiga materi, susunan, latar, dan monitor pindah sama persis; salinan JSON byte-identik. Folder data asli tidak dipakai.
- Ukuran `lentera.exe`: 12.350.976 byte (0.2.1: 11.176.448 byte, +1,1 MB untuk SQLite). `lentera-win32.exe`: 3.408.896 byte.
- Puncak working set selama uji alur (diambil tiap 30 ms; kasar):

| Data | Versi ini | 0.2.1 terpasang |
|---|---:|---:|
| Folder kosong | 36,8 MiB | 43,6 MiB |
| 31.000 ayat, saat pemindahan | 45,4 MiB | 52,9 MiB (JSON) |
| 31.000 ayat, dibuka ulang dari SQLite | 47,1 MiB | — |
| Win32, folder kosong / 31.000 ayat | 16,3 / 30,2 MiB | — |

  0.2.1 dijalankan dari folder instalasinya, jadi angkanya tidak sepenuhnya setara. Kesimpulannya hanya: tidak terlihat kenaikan RAM.

Belum diuji: macOS (menunggu CI), pembaruan dari 0.2.1 terpasang ke versi ini dengan data asli, folder data di flashdisk atau folder jaringan, dan listrik padam saat menyimpan (hanya dijamin oleh transaksi SQLite, tidak disimulasikan).
