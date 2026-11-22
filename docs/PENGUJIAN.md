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

## Pilih profil — 19 November 2026

Belum dirilis. Diuji di komputer pengembangan yang sama (Windows 11, dua monitor).

- `cargo test --workspace --locked`: 26 lulus di `lentera-core`, termasuk 4 tes baru: profil baru kosong dan menyalin latar serta monitor; aturan nama (wajib diisi, paling panjang 40 karakter, unik tanpa membedakan huruf besar/kecil, spasi dirapikan, id berisi path ditolak); `Store::open` membuka profil yang terakhir dibuka; hapus profil memindahkan folder ke `profiles-deleted` dan profil terakhir tidak bisa dihapus. `cargo clippy` untuk `lentera-core`, `lentera-win32`, `lentera-app`, dan `lentera-app` dengan `snapshot` (`-D warnings`) serta `cargo fmt --all --check` bersih.
- Uji alur UI `lentera` (build rilis dengan `snapshot`) di folder kosong: 17/17, termasuk 6 pemeriksaan baru: daftar profil, membuat profil "  Komisi Pemuda ✓ " (nama dirapikan, pustaka kosong, tayangan berhenti, monitor tetap), dua kartu profil, ganti nama Profil utama menjadi "Ibadah Umum", berganti kembali ke profil itu (materi hasil uji edit masih ada), dan menghapus profil Komisi Pemuda. `lentera-win32`: lulus.
- Tangkapan layar diperiksa: layar **Pilih profil**, dan bilah atas pada 1024×640. Pada lebar itu tombol profil hanya menampilkan lingkaran dan tombol **Tutup proyektor** serta **Impor** tidak terpotong.
- Dibuka manual dengan dua profil (folder profil yang dihapus uji alur dipindahkan kembali dari `profiles-deleted` ke `profiles`): layar **Pilih profil** langsung muncul, profil terakhir dibuka (Ibadah Umum) bertanda "Sedang dibuka", dan judul jendela "Lentera 0.2.1 · Ibadah Umum". Profil yang dipulihkan tampil lagi.

Belum diuji: dialog konfirmasi (ganti profil saat tayang, hapus profil) yang dilewati uji alur, navigasi layar profil dengan keyboard saja dan pembaca layar, macOS (menunggu CI), serta data asli pengguna dengan beberapa profil.

## Server login dan sinkron — 19 November 2026

Belum dipakai aplikasi dan belum di-deploy. Diuji di komputer pengembangan (Windows 11) dengan Go 1.26.5 dan PostgreSQL 18.4 lokal (cluster sementara di port 54331, bukan layanan PostgreSQL yang terpasang).

- `go vet ./...` dan `gofmt -l` bersih. `go test -count=1 ./...` dengan `TEST_DATABASE_URL`: 21 tes lulus.
  - Login: kode dikirim sekali dalam 60 detik, kode salah mengurangi sisa percobaan, 5 kali salah membuang kode, kode hanya bisa dipakai sekali, sesi kedaluwarsa ditolak, sesi lama diperpanjang saat dipakai, akun nonaktif tidak bisa masuk, email gagal tidak menahan percobaan berikutnya.
  - Profil: dibuat dengan id dari aplikasi, ganti nama yang lebih lama kalah, id milik akun lain ditolak, aturan nama, daftar per akun.
  - Sinkron: dua perangkat saling menerima materi, susunan, dan latar; tanda hapus tanpa judul dan isi; perubahan terbaru menang dan perangkat yang kalah menerima versi pemenang; kiriman sendiri tidak dikirim balik; waktu lebih dari 10 menit di depan jam server diganti; susunan yang menyebut materi tak dikenal ditolak tanpa mengubah apa pun; halaman per jumlah dan ukuran; 8 perangkat bersamaan × 25 kiriman menghasilkan 200 revisi unik; peran `viewer` / `editor` / `owner`, profil terhapus (410), cursor di depan server (409), dan kiriman tidak valid ditolak utuh.
  - HTTP: alur lengkap dari kode email sampai logout lewat endpoint `/v1`, JSON rusak (400), permintaan terlalu besar (413), dan batas laju (429).
- Program `lentera-server.exe` (12,5 MB, `CGO_ENABLED=0`) dijalankan manual: `migrate up` / `status`, lalu `serve`. Lewat `curl`: health, minta kode (kode muncul di log), verifikasi, buat profil, sinkron satu materi, daftar profil dengan revisi 1, dan `401` tanpa token. Token tidak tercatat di log.

Belum diuji: `go test -race` (butuh cgo; berjalan di CI Linux), pengiriman email lewat Resend, server di belakang Caddy, beban dengan 31.000 ayat lewat HTTP, dan aplikasi Lentera sebagai klien.

## Alkitab — 21 November 2026

Belum dirilis. Diuji di komputer pengembangan yang sama (Windows 11, dua monitor), hanya dengan teks buatan. Tidak ada terjemahan Alkitab asli yang dipakai.

- `cargo test --workspace --locked`: 50 lulus di `lentera-core`, termasuk 24 tes baru:
  - Kitab: tabel 66 kitab lengkap dengan nama dan singkatan unik; nama Indonesia, Inggris, singkatan TB, dan angka Romawi; awal nama harus menunjuk satu kitab (`Fil` → Filipi, `Ye` menyebut tiga pilihan).
  - Referensi: satu ayat, rentang, lintas pasal, koma, titik koma, satu dan beberapa pasal, kitab satu pasal (`Yud 3`), nama kitab saja, referensi yang masih diketik (`Yoh 3:`), pesan galat, label yang rapi, dan rentang yang dipendekkan sampai ayat terakhir.
  - Impor: OpenSong menurut nama kitab dengan ayat gabungan `5-6` dan kitab di luar 66 dilewati; OpenSong dengan 66 nama tak dikenal memakai urutan; Zefania dengan nomor kitab, bagian `INFORMATION`, dan catatan kaki dibuang; TSV dua bentuk dengan BOM dan baris judul; galat yang menyebut nomor baris.
  - `alkitab.db`: tambah, singkatan ganda ditolak, buka ulang, lookup rentang / daftar / ayat gabungan, galat (kitab tidak ada, pasal dan ayat di luar jangkauan), hapus terjemahan beserta ayatnya, dan skema yang lebih baru ditolak.
  - Slide: nomor ayat kecil, 1–3 ayat per slide dengan slide baru di pasal baru, ayat panjang mendapat slide sendiri, dan pemotongan baris tidak menghilangkan kata.
  - Materi sementara tampil seperti materi biasa tetapi tidak tersimpan di pustaka, dan id-nya tidak dipakai ulang.
- `cargo clippy` untuk `lentera-core`, `lentera-win32`, `lentera-app`, dan `lentera-app` dengan `snapshot` (`-D warnings`) serta `cargo fmt --all --check` bersih.
- Uji alur UI `lentera`: 24/24 di build debug dengan `snapshot` dan di build rilis, termasuk 7 pemeriksaan baru: tab Alkitab kosong; membaca berkas TSV buatan (36 ayat, singkatan usulan "AU"); menyimpannya sebagai "UJI"; `yoh 3:16-18` menampilkan slide dan daftar 36 ayat; Shift+klik ayat 20 menjadi `Yoh 3:16-20` dengan 5 ayat tersorot; menayangkannya tanpa masuk pustaka; **+ Susunan** menyimpan "Yohanes 3:16-20 (UJI)" ke pustaka dan susunan, dan layar tayang pindah ke materi yang tersimpan. `lentera-win32`: lulus.
- Program rilis 12,8 MB (12.792.832 byte). `THIRD-PARTY-NOTICES.txt` tidak berubah karena `roxmltree` sudah dipakai Slint.
- Kecepatan, build rilis, dengan teks sintetis 33.575 ayat dalam 66 kitab:

  | Langkah | Waktu |
  |---|---:|
  | Membaca OpenSong XML 3,9 MB / Zefania XML 4,4 MB | 24 / 26 ms |
  | Menyimpan satu terjemahan ke `alkitab.db` | 42 ms |
  | 100 kali lookup referensi | 22 ms |
  | Satu lookup 300 ayat | 0,27 ms |
  | Menghapus terjemahan (termasuk `VACUUM`) | 82 ms |

  `alkitab.db` berisi dua terjemahan sintetis berukuran 9,0 MB.
- Tangkapan layar diperiksa: tab Alkitab kosong, form impor, tab dengan `Yoh 3:16-18` dan daftar ayat, serta slide proyektor dengan nomor ayat kecil. Pada 1024×640 daftar ayat sempit tetapi masih terbaca.

Belum diuji: berkas Alkitab asli (OpenSong dan Zefania dari sumber lain bisa berbeda strukturnya), dialog pilih berkas dan konfirmasi hapus terjemahan yang dilewati uji alur, proyektor 4:3, daftar ayat dengan keyboard saja dan pembaca layar, pemakaian RAM dengan Alkitab lengkap, dan macOS (menunggu CI).

## Rilis 0.3.0 — 21 November 2026

Berisi penyimpanan SQLite per profil, layar Pilih profil, dan tab Alkitab dari bagian-bagian di atas. Diuji di komputer pengembangan yang sama (Windows 11).

- Versi workspace 0.3.0 (`Cargo.lock`: 3 crate Lentera) dan `app.manifest` 0.3.0.0.
- `cargo test --workspace --locked`: 50 lulus. `cargo clippy` untuk `lentera-core`, `lentera-win32`, dan `lentera-app` (`-D warnings`) serta `cargo fmt --all --check` bersih. `THIRD-PARTY-NOTICES.txt` disusun ulang: 300 crate, tidak berubah.
- Build rilis `lentera.exe`: 12.792.832 byte (0.2.1: 11.176.448 byte). FileVersion / ProductVersion 0.3.0, FileDescription "Lentera".
- Uji alur UI build rilis di folder kosong: 24/24, versi 0.3.0. `lentera-win32`: 9/9.
- **Pemindahan data asli.** Salinan `library.json` dari Lentera 0.2.1 yang terpasang di komputer ini (4 materi, 1 baris susunan) dibuka dengan `--data-dir` pada folder salinan:
  - Judul jendela "Lentera 0.3.0 · Profil utama", lalu ditutup normal (kode keluar 0).
  - `profiles/<id>/library.db` dibuat. Keempat materi sama persis (id, jenis, judul, isi), begitu juga susunan, latar, dan pilihan monitor. UUID unik, `PRAGMA integrity_check` = ok, `user_version` 1.
  - `library.json` tidak berubah (hash sama).
  - Dibuka kedua kali: profil yang sama terbuka, `library.db.bak` dibuat, tidak ada profil ganda.

Belum diuji: pembaruan otomatis 0.2.1 → 0.3.0 lewat GitHub Releases di instalasi asli (baru bisa setelah rilis diterbitkan), macOS (menunggu CI), dan tab Alkitab dengan berkas terjemahan asli.

## Impor EasyWorship — 22 November 2026

Belum dirilis. Diuji di komputer pengembangan yang sama (Windows 11, dua monitor) dengan salinan data EasyWorship milik pengembang: EasyWorship 2009 dan 7 dari ekspor di SSD eksternal, serta EasyWorship 8 yang terpasang di komputer ini. Data asli hanya dibaca dari salinan; teks lagu dan Alkitabnya tidak masuk repositori.

- `cargo test -p lentera-core --locked`: 66 lulus, termasuk 16 tes baru:
  - RTF EasyWorship 6–8: slide dipisah penanda `\sdslidemarker`, label bagian tersembunyi dibuang tetapi baris lirik tersembunyi tetap ikut, `\uN` termasuk pasangan surrogate, dan karakter Windows-1252. Label dikenali ("End", "Verse2", "end 1"), bukan baris lirik ("Chorus 2x", "Endless").
  - Lagu EasyWorship 6–8 (SQLite buatan, di folder bernama `ew7 #1`): dibaca lewat `Songs.db`, `SongWords.db`, atau foldernya, tanpa membuat berkas jurnal; pesan yang jelas bila `SongWords.db` tidak ada.
  - Lagu EasyWorship 2009: tabel Paradox buatan dengan memo di dalam baris dan di berkas `.MB`.
  - Alkitab `.ewb` 6–8 (letak ayat dalam satuan UTF-16, termasuk emoji; ayat rusak dan kitab di luar 66 dilewati) dan 2009 (biner + zlib, ayat yang berlanjut ke baris berikutnya).
  - Berkas lain ditolak dengan pesan yang jelas.
  - Rencana impor: lagu yang sudah ada dilewati (tanda baca dan huruf besar/kecil diabaikan), judul kembar diberi nomor, impor ulang tidak menambah apa pun.
- `cargo clippy` untuk `lentera-core`, `lentera-app`, dan `lentera-win32` (`-D warnings`) serta `cargo fmt --all --check` bersih. `miniz_oxide` (zlib) sudah tercantum di `THIRD-PARTY-NOTICES.txt`.
- Uji alur UI: 25/25 di build `ci` dengan `snapshot` dan di build rilis, termasuk pemeriksaan baru `songs_imported`: tiga lagu masuk lewat jalur impor yang sama, satu dilewati karena sudah ada, satu menjadi "Lagu Impor Uji (2)", tersimpan di `library.db`, dan lagu pertama terpilih. Tangkapan menu **Impor** pada jendela 1024×640: keempat baris dan petunjuknya tidak terpotong.
- Program rilis 12,9 MB (12.889.088 byte; 0.3.0: 12.792.832 byte).
- **Lagu, data asli** (build rilis):

  | Sumber | Lagu berlirik | Tanpa lirik | Waktu baca |
  |---|---:|---:|---:|
  | EasyWorship 2009 (Paradox) | 510 | 7 | 33 ms |
  | EasyWorship 7 (SQLite + RTF) | 483 | 7 | 26 ms |
  | EasyWorship 8 terpasang | 0 | 0 | <1 ms |

  - Dibandingkan dengan ekspor teks yang dibuat terpisah dari database yang sama: 464 dari 510 lagu EasyWorship 2009 dan 439 dari 483 lagu EasyWorship 7 sama isinya (tanda baca, spasi, dan huruf besar/kecil diabaikan). Semua 90 sisanya hanya berbeda karena label yang dibuang ("End", "Verse 1–3", "Bridge"); tidak ada kata lirik yang hilang atau bertambah.
  - Impor ke pustaka kosong: EasyWorship 2009 menambah 509 lagu (1 lagu muncul dua kali di database). Lalu EasyWorship 7: 91 lagu baru (35 di antaranya diberi nomor karena liriknya berbeda dari lagu 2009 berjudul sama), 392 sudah ada dan dilewati. Kedua impor diulang: 0 lagu baru.
- **Alkitab, data asli** (build rilis):

  | Berkas | Ayat | Dilewati | Waktu baca |
  |---|---:|---:|---:|
  | TB, EasyWorship 7 | 31.172 | 0 | 76 ms |
  | KJV, EasyWorship 8 | 31.102 | 0 | 72 ms |
  | KJV, EasyWorship 2009 | 31.102 | 0 | 60 ms |
  | TB, EasyWorship 2009 | 31.102 | 21 | 62 ms |

  TB (EasyWorship 7), KJV (EasyWorship 8), dan KJV (EasyWorship 2009) sama ayat per ayat dengan ekspor. Nama dan singkatan terbaca dari berkas 6–8 ("Alkitab Terjemahan Baru" / TB, "King James Version" / KJV). TB EasyWorship 2009 adalah edisi lain (tanda kutip lurus, susunan ayat berbeda); 21 nomor ayat ganda dilewati.
- Folder EasyWorship di komputer ini terdeteksi: `C:\Users\Public\Documents\Softouch\EasyWorship\Default\v6.1\Databases\Data\Songs.db` dan `C:\ProgramData\Softouch\Easyworship.v8\Plugins\Bibles`. Database EasyWorship memakai jurnal biasa (bukan WAL), jadi membaca dengan `immutable=1` mendapat semua data yang sudah disimpan EasyWorship.

- **Dialog bawaan Windows, setelah uji pertama pengguna.** Pada uji pertama Lentera macet setelah `Songs.db` dipilih. Klik dikirim sebagai pesan jendela (`PostMessage`, tanpa menggerakkan mouse) ke build uji di folder data kosong, dan gambar jendela ditangkap dengan `PrintWindow`:
  - Sebelum perbaikan: **Impor → Lagu dari EasyWorship** membuka dialog berkas dengan benar. Setelah `F:\…\ew7-databases\Songs.db` dipilih, dialog konfirmasi "Impor lagu EasyWorship" sudah dibuat tetapi tidak pernah terlihat, jendela Lentera tetap nonaktif, dan proses memakai ±80% satu inti CPU. Penyebab: setelah dialog berkas tertutup, jendela Lentera perlu digambar ulang. Selama callback klik masih berjalan, winit terus mengulang permintaan gambar ulang, jadi antrean pesan Windows tidak pernah kosong, padahal MessageBox baru tampil saat antrean kosong. Uji alur otomatis tidak menangkap ini karena tidak memakai dialog berkas.
  - Perbaikan: berkas yang dipilih diproses 50 ms setelah callback selesai (`pick_file_then`), untuk keempat impor (lirik, ayat TSV, lagu EasyWorship, Alkitab).
  - Sesudah perbaikan, lewat klik di menu: konfirmasi tampil di depan dengan jumlah yang benar (483 ditemukan, 479 baru, 4 sudah ada, 6 diberi nomor, 7 tanpa lirik). **Yes** mengimpor 479 lagu dan menampilkan "479 lagu EasyWorship diimpor.", dan CPU kembali diam. Memilih `README.md` menampilkan pesan "Impor EasyWorship gagal" di depan. **Impor → Alkitab lengkap** dengan `TB.ewb` membuka formulir (66 kitab, 31.172 ayat; TB / Alkitab Terjemahan Baru), lalu **Simpan** menulis "TB tersimpan: 31.172 ayat."
  - Diperiksa ulang: fmt, clippy (core, app, app dengan `snapshot`, win32), 66 tes core, uji alur UI 25/25.

Belum diuji: klik dengan mouse sungguhan di komputer lain, EasyWorship 6 asli, dan database EasyWorship dari gereja lain (termasuk lagu dengan format RTF lain).

## Pustaka lebih dari 500 materi — 22 November 2026

Belum dirilis. Sebelumnya daftar pustaka berhenti di 500 materi pertama (urut masuk), jadi setelah mengimpor ratusan lagu sisanya hanya bisa ditemukan lewat pencarian. Sekarang semua materi tampil.

Diukur di komputer pengembangan (Intel i5-13500, 32 GB, Windows 11) dengan build rilis (`lto`, `opt-level = "s"`), renderer software, jendela operator 1360×840 pada skala 100%, dan folder `--data-dir` sementara. Data: 600, 2.000, dan 5.000 lagu sintetis, rata-rata 695 huruf dan 9 slide per lagu (sekitar dua kali rata-rata lagu EasyWorship asli yang diperiksa: 355 huruf, 8,6 slide). Mode ukur sementara (tidak ikut di-commit) memanggil `refresh_library`, lalu menggambar seluruh jendela operator dengan `Window::take_snapshot`. Ini batas atas satu frame karena tanpa partial rendering. Pengukuran meliputi mengetik "kasih setia" huruf demi huruf lalu menghapusnya, mengetik "zzzz", dan menggulir daftar sampai bawah dengan roda mouse (2.600 px per langkah). Memori dibaca dengan `GetProcessMemoryInfo`. Setiap angka adalah median dari 5 kali buka.

`refresh_library` untuk daftar penuh (kotak cari kosong):

| Lagu | Batas 500 (lama) | Tanpa batas, sebelum perbaikan | Tanpa batas (sekarang) |
|---:|---:|---:|---:|
| 600 | 1,64 ms (500 baris) | 1,89 ms | 0,58 ms |
| 2.000 | 1,55 ms (500 baris) | 5,98 ms | 1,78 ms |
| 5.000 | 1,62 ms (500 baris) | 14,97 ms | 4,34 ms |

Tanpa batas (sekarang):

| Lagu | Mengetik: `refresh_library` terlama | Mengetik: refresh + gambar jendela, median / terlama | Gulir sampai bawah: langkah, gambar median / terlama | Peak working set (batas 500 → sekarang) |
|---:|---:|---:|---:|---:|
| 2 (contoh) | 0,04 ms | 10,6 / 11,6 ms | — | 36,4 → 36,5 MiB |
| 600 | 0,85 ms | 12,0 / 13,1 ms | 13, 11,1 / 13,8 ms | 38,3 → 38,4 MiB |
| 2.000 | 2,79 ms | 13,6 / 14,7 ms | 41, 11,0 / 12,2 ms | 41,3 → 42,0 MiB |
| 5.000 | 6,68 ms | 16,7 / 18,7 ms | 101, 10,9 / 12,3 ms | 43,9 → 45,6 MiB |

Membuka profil (membaca `library.db`) 9 / 14 / 22 ms untuk 600 / 2.000 / 5.000 lagu. Pembukaan pertama, yang juga memindahkan `library.json`, 10 / 20 / 36 ms.

Temuan:

- **Daftar Slint tervirtualisasi, juga dengan renderer software.** `for` yang langsung berada di dalam `ListView` dikompilasi Slint 1.19 sebagai repeater ListView (`update_visible_instances` di `i-slint-core`, tidak bergantung pada renderer), jadi hanya baris yang terlihat yang dibuat. Hasil ukur cocok: menggambar jendela tetap ±11 ms untuk 2 maupun 5.000 baris, juga saat digulir sampai baris ke-5.000, dan working set hanya naik 0,6 MiB selama mengetik dan menggulir 5.000 baris.
- **Biaya terbesar ada di pembuatan baris, bukan di pencarian.** Pencarian dengan kotak kosong hanya 0,03 ms untuk 5.000 lagu. Keterangan "Lirik · 9 slide" dulu dihitung dengan memotong seluruh isi setiap lagu menjadi slide. Sekarang `Item::slide_count` menghitungnya per byte tanpa menyalin teks (2,9 ms untuk 5.000 lagu), sehingga daftar penuh turun dari 15,0 ke 4,3 ms.
- Huruf pertama yang diketik paling berat karena hampir semua lagu cocok: `refresh_library` 6,1 ms untuk 5.000 lagu. Teks yang tidak cocok sama sekali ("zzzz") 3,6 ms; ini biaya mengubah seluruh isi pustaka ke huruf kecil.
- Memori bertambah 1,8 MiB untuk 5.000 lagu dibanding batas 500. Sebagian besar kenaikan dibanding pustaka contoh adalah isi pustaka itu sendiri.

Keputusan: batas 500 dihapus. Urutan tetap urutan masuk. Jumlah di atas daftar memakai titik ribuan ("5.000 materi", "12 dari 600").

Verifikasi:

- `cargo test -p lentera-core --locked`: 52 lulus, termasuk 2 tes baru. Tes pertama memastikan semua 1.201 materi tampil urut dan lagu ke-1.100 bisa dicari. Tes kedua memastikan `slide_count` sama dengan `slides().len()` untuk `\r\n`, `\r`, spasi Unicode, dan huruf non-ASCII.
- `cargo fmt --all --check` dan `cargo clippy` untuk `lentera-core`, `lentera-app` (dengan dan tanpa `snapshot`), dan `lentera-win32` (`--all-targets -D warnings`) bersih.
- Build rilis `lentera.exe` 12.792.832 byte (sama dengan 0.3.0). Uji alur UI di folder kosong: 24/24. `lentera-win32`: 9/9. Daftar Win32 lama juga kini menampilkan semua hasil; tampilan ini tidak dirilis.

Belum diuji: PC sasaran RAM 2–4 GB. Bila CPU-nya 4–6× lebih lambat per inti, satu ketikan di 5.000 lagu diperkirakan sekitar 25–40 ms ditambah waktu menggambar, dan di 600 lagu di bawah 5 ms; angka ini perkiraan, belum diukur. Juga belum diuji: macOS dan pustaka hasil impor EasyWorship asli.

## Rilis 0.4.0 — 22 November 2026

Berisi impor EasyWorship, perbaikan macet setelah dialog pilih berkas, dan pustaka tanpa batas 500 dari bagian-bagian di atas. Diuji di komputer pengembangan yang sama (Windows 11, dua monitor).

- Versi workspace 0.4.0 (`Cargo.lock`: 3 crate Lentera) dan `app.manifest` 0.4.0.0.
- `cargo test -p lentera-core --locked`: 68 lulus (66 dari impor EasyWorship dan 2 dari pustaka). `cargo fmt --all --check` dan `cargo clippy` untuk `lentera-core`, `lentera-app` (dengan dan tanpa `snapshot`), dan `lentera-win32` (`--all-targets -D warnings`) bersih. `THIRD-PARTY-NOTICES.txt` disusun ulang: 300 crate, tidak berubah.
- Build rilis `lentera.exe`: 12.891.136 byte (0.3.0: 12.792.832 byte). FileVersion / ProductVersion 0.4.0, FileDescription "Lentera".
- Uji alur UI build rilis di folder kosong: 25/25, versi 0.4.0. `lentera-win32` (profil `ci`): 9/9.
- **Lagu EasyWorship asli ke pustaka penuh.** Build rilis dengan folder data kosong; klik dikirim lewat `PostMessage`. **Impor → Lagu dari EasyWorship** dengan `Songs.DB` EasyWorship 2009 dari SSD, lalu `Songs.db` EasyWorship 7:
  - Konfirmasi pertama: 510 ditemukan, 509 baru, 1 sudah ada, 7 tanpa lirik. Kedua: 483 ditemukan, 91 baru, 392 sudah ada, 35 diberi nomor, 7 tanpa lirik. Sama dengan uji impor sebelumnya.
  - Pustaka menampilkan "602 materi" (600 lagu impor dan 2 contoh). Daftar digulir dengan roda mouse sampai bawah; materi terakhir, "Yesus pegang erat tanganku" (urutan ke-602, sama dengan `library.db`), tampil. Di 0.3.0 materi setelah urutan ke-500 tidak muncul di daftar.
  - Jendela tetap responsif. `library.db`: 602 materi, `PRAGMA integrity_check` = ok. Working set 70 MiB (puncak 84 MiB) setelah dua dialog berkas Windows, kedua impor, dan menggulir. Dialog berkas memuat komponen shell Windows, jadi angka ini tidak sebanding dengan pengukuran pustaka di atas.

Belum diuji: pembaruan otomatis 0.3.0 → 0.4.0 lewat GitHub Releases di instalasi asli (baru bisa setelah rilis diterbitkan), macOS (menunggu CI), dan PC sasaran dengan RAM 2–4 GB.

## Latar gambar — 22 November 2026

Belum dirilis. Satu gambar dipakai sebagai latar semua slide, dipilih dari galeri per profil. Latar per lagu atau ayat belum ada. Diuji di komputer pengembangan yang sama (Windows 11, dua monitor; monitor kedua tegak 1080×1920).

Cara kerja:

- Gambar disalin ke `profiles\<id profil>\backgrounds\<nama>.jpg` (JPEG kualitas 88), dengan thumbnail 192×108 di `backgrounds\thumbs`. Database profil hanya menyimpan nama berkas (`meta.background_image`); nama yang menunjuk ke luar folder latar diabaikan saat dibaca.
- Gambar diperkecil secukupnya untuk menutupi 1920×1080 dan tidak pernah diperbesar; sisi terpanjang paling besar 3840. Orientasi EXIF diterapkan, dan bagian transparan PNG diratakan di atas hitam. Berkas sumber paling besar 40 MB, dan memori dekode dibatasi 320 MB.
- Gambar yang isinya sama persis hanya disimpan sekali. Nama yang sudah dipakai diberi nomor "(2)", dan ekstensi gambar di akhir nama dibuang ("x.jpg.jpeg" menjadi "x.jpg").
- Gambar ditambahkan di thread terpisah. Selama itu status menampilkan "Menambahkan gambar latar… n dari N".
- Teks di atas gambar putih dengan garis tepi hitam 72% setebal 7% ukuran huruf (`stroke-style: outside`). Gambarnya digelapkan 35%.
- Crate `image` 0.25 (fitur `jpeg` dan `png`) sudah ikut di aplikasi lewat Slint. `Cargo.lock` tidak bertambah crate, dan `THIRD-PARTY-NOTICES.txt` yang disusun ulang tetap 300 crate, tanpa perubahan.

Verifikasi:

- `cargo test -p lentera-core --locked`: 78 lulus, termasuk 10 tes baru. Tes baru mencakup: foto besar diperkecil, gambar kecil atau tegak tidak diperbesar, PNG transparan, gambar ganda dan nama bentrok, pembersihan nama, penolakan nama di luar folder, pesan galat untuk berkas rusak dan format lain, thumbnail yang hilang dibuat ulang, salin dan hapus, serta penyimpanan nama latar di database profil.
- `cargo fmt --all --check` dan `cargo clippy` untuk `lentera-core`, `lentera-app` (dengan dan tanpa `snapshot`), dan `lentera-win32` (`--all-targets -D warnings`) bersih.
- Build rilis `lentera.exe`: 13.290.496 byte (0.4.0: 12.891.136 byte).
- Uji alur UI build rilis di folder kosong: 29/29. Empat pemeriksaan baru membuat PNG 1600×900, menambahkannya, memastikan galeri menampilkannya, lalu menghapusnya. Tangkapan galeri, slide, dan proyektor diperiksa. `lentera-win32` (profil `ci`): 9/9.
- **Gambar asli.** 24 berkas JPG, JPEG, dan JFIF dari folder ekspor, 3 KB sampai 5,5 MB, 318×159 sampai 8250×4641, termasuk gambar tegak. `background::add` dijalankan per berkas di build rilis, lewat program ukur sementara yang tidak di-commit:
  - Semua berhasil. Waktu tambah median 48 ms, terlama 201 ms (6000×3376, 5,5 MB).
  - Membuka salinan tersimpan (`load`) butuh 6–14 ms.
  - Salinan gambar yang tidak perlu diperkecil bisa sedikit lebih besar dari aslinya karena disimpan ulang dengan kualitas 88, misalnya 1600×900: 385 KB menjadi 441 KB.
- **Aplikasi.** Build `ci`, lalu build rilis, dengan folder data kosong; klik dikirim lewat `PostMessage`.
  - Galeri kosong → **Tambah gambar** → empat gambar dipilih sekaligus, termasuk `.jfif` dan gambar 3 KB. Hasilnya "4 gambar latar ditambahkan." dan empat thumbnail.
  - Setelah gambar diklik, slide, pratinjau, layar tayang, proyektor, dan kotak **Gambar** di **Latar layar** memakai gambar itu, dan status menampilkan "Latar: …". Di monitor tegak, proyektor memotong gambar 16:9 rata tengah.
  - Pada gambar yang hampir putih dan pada gambar ramai yang penuh tulisan, teks putih bergaris tepi tetap terbaca.
  - Hapus: **No** membiarkan gambar. **Yes** menghapus salinan dan thumbnail, sedangkan berkas aslinya tetap ada; status "Latar … dihapus.".
- **Memori.** Build rilis, jendela 1360×840, proyektor terbuka dan sedang tayang, 3 kali buka:
  - Dengan latar warna, working set 35,7 MiB (private 16,7 MiB). Dengan gambar 1920×1080 terpilih, 42,1 MiB (private 22,9 MiB). Selisih 6,4 MiB itu sesuai dengan satu gambar RGB 1920×1080 (5,9 MiB).
  - Menambah foto 6000×3376 lewat dialog berkas sempat mencapai puncak 139,6 MiB, lalu turun ke 81,6 MiB. Angka puncak ini termasuk komponen dialog berkas Windows dan dekode sementara foto (sekitar 58 MiB untuk pikselnya).

Belum diuji:

- PC sasaran dengan RAM 2–4 GB. Dekode foto besar butuh memori sementara sebesar ukuran pikselnya.
- Kecerahan dan keterbacaan di proyektor sebenarnya.
- macOS.
- PNG asli (baru PNG buatan tes) dan JPEG CMYK.

## Latar per materi — 22 November 2026

Belum dirilis. Setiap materi pustaka bisa memakai gambar latar sendiri. Materi tanpa gambar sendiri memakai latar umum (warna atau gambar, seperti di bagian "Latar gambar" di atas). Diuji di komputer pengembangan yang sama.

Cara kerja:

- `Item.background` berisi nama gambar di folder `backgrounds` profil, atau `None` untuk latar umum. Nilainya disimpan di kolom baru `items.background`, dan mengubahnya juga memperbarui `updated_at` materi. Nama yang menunjuk ke luar folder latar diabaikan saat dibaca.
- Skema profil naik ke versi 2. Profil versi 1 (Lentera 0.3.0–0.4.0) diperbarui saat dibuka dengan `ALTER TABLE items ADD COLUMN background TEXT` dalam satu transaksi, sebelum pustaka dibaca. Cadangan `library.db.bak` dibuat sesudahnya, jadi juga berskema 2. Lentera 0.4.0 menolak profil versi 2 dengan pesan "dibuat oleh Lentera versi yang lebih baru"; penolakan versi yang lebih baru sudah diuji di tes unit.
- Aplikasi menghitung dua latar terpisah:
  - latar materi terpilih, untuk slide di tengah, pratinjau, dan hasil editor;
  - latar materi yang tayang, untuk layar Tayang dan proyektor.
- Materi sementara (ayat Alkitab yang belum di pustaka) selalu memakai latar umum. Tombol latar menyimpannya ke pustaka dulu, seperti **Edit materi**.
- Hanya gambar yang sedang dipakai (latar umum, materi terpilih, materi tayang) yang disimpan di memori, jadi paling banyak tiga gambar. Gambar materi yang hilang atau rusak dilepas dari materi itu, dengan pesan.
- Menghapus gambar dari galeri melepasnya dari semua materi dan dari latar umum. Dialog konfirmasi menyebut keduanya dan jumlah materinya.
- Tombol latar berupa chip kecil di samping keterangan di atas judul: "Latar umum" atau "Latar: nama". Di lebar minimum 1024 px teksnya dipendekkan, sedangkan **Edit materi** kini tidak pernah dipotong.

Verifikasi:

- `cargo test -p lentera-core --locked`: 80 tes lulus, termasuk 2 tes baru:
  - latar per materi di `Library`: pasang, lepas, hitung pemakai, materi sementara, dan `library.json` lama;
  - simpan dan baca latar materi, nama di luar folder, serta pembaruan profil skema 1 ke 2 dengan isi tetap.
- `cargo fmt --all --check` dan `cargo clippy` untuk `lentera-core`, `lentera-app` (dengan dan tanpa `snapshot`), dan `lentera-win32` (`--all-targets -D warnings`) bersih.
- Build rilis `lentera.exe`: 13.324.288 byte (0.4.0: 12.891.136 byte; dengan latar umum saja: 13.290.496 byte).
- Uji alur UI build rilis di folder kosong: 31/31. Dua pemeriksaan baru, dan pemeriksaan hapus diperluas:
  - materi terpilih diberi gambar sendiri saat latar umum berupa warna;
  - materi itu ditayangkan, lalu dikembalikan ke latar umum;
  - menghapus gambar yang menjadi latar umum sekaligus latar materi melepas keduanya dan mengosongkan memori gambar.
- `lentera-win32` (profil `ci`): 9/9.
- **Aplikasi** (build `ci`, jendela 1360×840, folder data kosong, 2 gambar asli; klik lewat `PostMessage`):
  - Chip "Latar umum" membuka galeri **Latar materi** untuk lagu contoh. Setelah gambar diklik, slide dan pratinjau memakai gambar itu, layar Tayang yang kosong tetap memakai latar umum, status "Latar materi ini: …", dan tombol **Pakai latar umum** muncul.
  - Lagu ditayangkan, lalu ayat dipilih dan latar umum diganti gambar lain. Ayat memakai latar umum yang baru, sedangkan lagu yang tayang tetap memakai gambarnya sendiri di layar Tayang dan proyektor.
  - Setelah **Berikutnya** dua kali, tayangan pindah ke ayat dan latarnya ikut menjadi latar umum.
  - Setelah aplikasi dibuka ulang, lagu tetap "Latar: …".
  - Gambar yang menjadi latar umum dan latar lagu dihapus. Dialog menyebut keduanya; lagu kembali ke latar umum, latar umum kembali ke warna, dan berkasnya hilang dari folder.
  - Temuan saat uji, sudah diperbaiki:
    - chip awalnya melebar memenuhi baris;
    - keterangan galeri latar umum tiga baris dan mendorong gambar ke bawah, kini dua baris;
    - di 1024 px **Edit materi** terpotong.

Belum diuji:

- Memori saat latar umum, materi terpilih, dan materi tayang memakai tiga gambar berbeda. Perkiraannya sekitar 6 MiB per gambar; belum diukur.
- Membuka profil berskema 2 dengan Lentera 0.4.0 (kemudian diuji di bagian "Rilis 0.5.0").
- PC dengan RAM 2–4 GB, proyektor sebenarnya, dan macOS.

## Rilis 0.5.0 — 22 November 2026

Berisi latar umum berupa gambar dan latar per materi dari dua bagian di atas. Diuji di komputer pengembangan yang sama.

- Versi workspace 0.5.0 (`Cargo.lock`: 3 crate Lentera) dan `app.manifest` 0.5.0.0.
- `cargo test -p lentera-core --locked`: 80 lulus. `cargo fmt --all --check` dan `cargo clippy` untuk `lentera-core`, `lentera-app` (dengan dan tanpa `snapshot`), dan `lentera-win32` (`--all-targets -D warnings`) bersih. `THIRD-PARTY-NOTICES.txt` disusun ulang: 300 crate, tidak berubah.
- Build rilis `lentera.exe`: 13.324.288 byte (0.4.0: 12.891.136 byte). FileVersion / ProductVersion 0.5.0, FileDescription "Lentera".
- Uji alur UI build rilis di folder kosong: 31/31, versi 0.5.0. `lentera-win32` (profil `ci`): 9/9.
- **Profil 0.4.0 dibuka 0.5.0.** Salinan `lentera.exe` 0.4.0 dari instalasi (instalasinya tidak diubah) membuat profil di folder data uji lewat uji alurnya: 5 materi dan susunan 3 materi, skema 1. Build rilis 0.5.0 lalu membuka folder itu:
  - Profil terbuka dengan semua materi dan susunan ibadah, dan chip menunjukkan "Latar umum".
  - `library.db` dan `library.db.bak` sama-sama skema 2 dengan kolom `background`. Kelima materi tetap ada, dan `PRAGMA integrity_check` = ok.
  - Pembaruan ini diulang dari profil 0.4.0 baru dua kali lagi, masing-masing ditutup dengan cara berbeda, dan hasilnya sama.
  - Salinan 0.4.0 yang membuka profil hasil pembaruan menampilkan "Lentera tidak bisa dibuka … dibuat oleh Lentera versi yang lebih baru. Perbarui Lentera." Profilnya tidak berubah.
- **Menutup jendela.** 0.4.0 dan 0.5.0 ditutup lewat `SC_CLOSE` (seperti tombol X) dan `WM_CLOSE`, juga saat jendela diperkecil, setelah terbuka 70 detik, dan setelah pembaruan profil. Prosesnya keluar dalam 0,3 detik.
  - Satu kali, pada percobaan pertama, proses 0.5.0 tidak keluar setelah jendelanya hilang dan harus dihentikan paksa. Selama itu kunci folder data tetap dipegang.
  - Pesan tutup percobaan itu dikirim dari skrip yang berbeda. Kejadiannya tidak terulang dalam 12 percobaan berikutnya, termasuk urutan yang sama, jadi penyebabnya belum diketahui. Kode untuk menutup tidak berubah sejak 0.4.0.

Belum diuji:

- Pembaruan otomatis 0.4.0 → 0.5.0 lewat GitHub Releases di instalasi asli. Ini baru bisa diuji setelah rilis diterbitkan.
- macOS (menunggu CI).
- PC sasaran dengan RAM 2–4 GB.
