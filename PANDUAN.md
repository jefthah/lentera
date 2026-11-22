# Lentera 0.4 — presentasi ibadah ringan

Aplikasi presentasi ibadah berbasis Rust dan Slint untuk lirik, ayat, dan proyektor, untuk Windows dan macOS. Sasaran pengembangannya komputer dengan RAM 2–4 GB; ini sasaran, bukan hasil uji pada semua komputer lama.

## Memasang dan memperbarui

1. Unduh **LenteraApp-win-Setup.exe** dari halaman rilis GitHub, lalu jalankan. Tidak perlu hak admin. Karena aplikasi belum bertanda tangan digital, Windows mungkin menampilkan "Windows protected your PC": pilih **More info → Run anyway**.
2. Buka Lentera dari shortcut **Lentera** di Desktop atau Start Menu.
3. Saat dibuka dan tersambung ke internet, Lentera memeriksa versi baru dan mengunduhnya di latar belakang. Bila sudah siap, muncul tombol **Perbarui ke …** di kanan bawah.
   - Biarkan saja: pembaruan dipasang otomatis saat Lentera ditutup.
   - Atau klik tombolnya, lalu pilih **Yes**: Lentera ditutup, diperbarui, dan dibuka kembali. Proyektor ikut tertutup sebentar, jadi lakukan di luar ibadah.
4. Untuk menghapus: Settings → Apps → **Lentera** → Uninstall. Data di `%LOCALAPPDATA%\Lentera` tidak ikut terhapus.

Pengguna 0.1.1 sampai 0.3.0 menerima 0.4.0 lewat pembaruan otomatis di atas. Pengguna 0.1.0 (ZIP lama) cukup memasang installer sekali. Pengguna 0.2.1 atau sebelumnya: saat versi baru pertama kali dibuka, pustaka lama dipindahkan otomatis ke **Profil utama** (lihat **Data dan cadangan**).

Versi macOS belum punya installer; bangun dari kode sumber (lihat **Untuk pengembang**).

## Mengenal layar

- **Bilah atas:** tombol profil (lingkaran berhuruf awal dan nama profil yang sedang dibuka), kolom pencarian, tombol **Buka / Tutup proyektor**, pilihan monitor proyektor, menu **Impor**, dan tombol **ⓘ** (Tentang Lentera).
- **Kiri:** **Susunan ibadah** di atas, lalu tab **Pustaka** dan **Alkitab**. Di Pustaka, filter **Semua / Lagu / Ayat** menyaring materi. Tab Alkitab dijelaskan di bagian **Alkitab**.
- **Tengah:** semua slide materi terpilih. Slide berbingkai kuning sedang dipratinjau, slide berbingkai merah sedang tayang. Tombol latar di atas judul mengatur latar materi itu (lihat **Latar**).
- **Kanan:** **Pratinjau**, tombol **Tayangkan**, layar **Tayang** (yang sedang dilihat jemaat), tombol kontrol, dan **Latar umum**.
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
| Enter di kotak referensi Alkitab | Tayangkan ayat yang diketik |

## Latar

Slide, pratinjau, layar tayang, dan proyektor memakai **latar umum**, kecuali materi yang diberi gambar latar sendiri.

**Latar umum** ada di bawah kolom kanan: tiga warna atau satu gambar.

- Klik kotak **Gambar** untuk membuka galeri latar, lalu tekan **Tambah gambar**. Pilih satu atau beberapa gambar JPG, JPEG, JFIF, atau PNG (Ctrl+klik atau Shift+klik di pemilih berkas). Bila EasyWorship terpasang dengan gambar latar, pemilih berkas terbuka di folder gambar EasyWorship.
- Klik gambar di galeri untuk memakainya sebagai latar umum. Bila hanya satu gambar ditambahkan, gambar itu langsung dipakai. Klik salah satu warna untuk kembali ke latar warna; gambarnya tetap ada di galeri.

**Latar per materi:** sebuah lagu atau ayat bisa memakai gambarnya sendiri.

- Pilih materinya, lalu klik tombol latar di samping keterangan di atas judul. Tombol itu bertuliskan **Latar umum**, atau **Latar: nama gambar** bila materi sudah punya gambar sendiri. Galeri **Latar materi** terbuka untuk materi itu saja.
- Klik gambar untuk memakainya di materi itu. **Pakai latar umum** mengembalikan materi ke latar umum. Gambar yang ditambahkan dari galeri ini masuk ke galeri yang sama dengan latar umum.
- Ayat dari tab Alkitab yang belum ada di pustaka disimpan dulu ke pustaka, seperti saat diedit.
- Slide di tengah, pratinjau, dan hasil editor memakai latar materi yang dipilih. Layar **Tayang** dan proyektor memakai latar materi yang sedang tayang, jadi latarnya ikut berganti saat tayangan pindah ke materi berikutnya dalam susunan.
- Mengganti latar umum tidak mengubah materi yang punya gambar sendiri.

Untuk semua gambar latar:

- Di atas gambar, teks selalu putih dengan garis tepi gelap, dan gambarnya sedikit digelapkan supaya teks tetap terbaca. Pilih gambar yang tidak terlalu ramai di bagian tengah.
- Gambar mengisi seluruh layar. Bila bentuknya tidak sama dengan layar, bagian yang tidak muat dipotong rata tengah; untuk proyektor biasa, gambar 16:9 (misalnya 1920×1080) paling pas.
- Lentera menyimpan salinan gambar di folder profil (`backgrounds`), diperkecil seperlunya untuk layar 1920×1080. Gambar yang lebih kecil tidak diperbesar dan bisa terlihat kurang tajam di proyektor. Berkas asli tidak diubah.
- Tombol tempat sampah di gambar menghapusnya dari profil setelah konfirmasi. Berkas aslinya di komputer tidak ikut terhapus. Materi yang memakai gambar itu kembali ke latar umum, dan bila gambar itu latar umum, latar umum kembali ke warna.

Setiap profil punya galeri sendiri.

## Alkitab

Lentera tidak membawa teks Alkitab. Impor sekali terjemahan yang Anda berhak pakai; setelah itu ayat cukup ditayangkan dengan mengetik referensinya.

**Mengimpor terjemahan.** Buka tab **Alkitab** di kolom kiri lalu tekan **Impor Alkitab**, atau pilih menu **Impor → Alkitab lengkap**. Pilih berkas OpenSong XML, Zefania XML, TSV UTF-8, atau Alkitab EasyWorship `.ewb` (formatnya di bawah). Lentera menyebut jumlah kitab dan ayat yang terbaca. Periksa **Singkatan** (ditulis di judul ayat, misalnya TB; paling panjang 12 karakter dan belum dipakai terjemahan lain) dan **Nama terjemahan**, lalu tekan **Simpan**. Terjemahan berikutnya diimpor lewat ikon impor di samping pilihan terjemahan.

**Menayangkan ayat.**

1. Pilih terjemahan, lalu ketik referensi di kotak di bawahnya, misalnya `Yoh 3:16-18`. Slide ayatnya langsung tampil di tengah untuk dipratinjau, dan di bawah kotak tampil jumlah ayat dan slide.
2. Tekan **Enter** untuk menayangkan slide pertama, lalu lanjutkan dengan **Berikutnya** seperti materi lain.
3. Di bawahnya tampil seluruh ayat dalam pasal itu. Klik ayat untuk memilihnya, **Shift+klik** ayat lain untuk memilih rentang, atau klik dua kali untuk langsung menayangkan. Ayat yang dipilih tersorot dan referensinya ditulis di kotak.

| Ketik | Hasil |
|---|---|
| `Yoh 3:16` atau `Yoh 3.16` | Satu ayat |
| `Yoh 3:16-18` | Rentang ayat |
| `Yoh 3:16-4:2` | Rentang yang melewati pasal |
| `Rm 8:28, 31-39` | Beberapa bagian dalam satu pasal |
| `Yoh 3:16; 4:1` | Bagian dari beberapa pasal |
| `Mzm 23` / `Mzm 1-2` | Satu pasal / beberapa pasal penuh |
| `Yud 3` | Ayat 3, untuk kitab yang hanya satu pasal |
| `Yoh` | Pasal 1 tampil di daftar; ketik atau klik ayatnya |

Nama kitab boleh ditulis lengkap (`Yohanes`), dengan singkatan TB (`Kej`, `Mzm`, `1Kor`, `Why`), dengan angka Romawi (`II Kor`), dalam bahasa Inggris (`John`, `Ps`), atau dengan awal nama yang jelas (`Fil`). Huruf besar/kecil tidak berpengaruh. Bila awal nama bisa berarti beberapa kitab (misalnya `Ye`), Lentera menyebut pilihannya. Rentang yang melewati ayat terakhir dipendekkan (`Yoh 3:16-99` menjadi `Yoh 3:16-36`). Sekali tampil paling banyak 300 ayat.

**Slide ayat.** **Otomatis** membagi 1–3 ayat per slide menurut panjangnya, dengan nomor ayat kecil di depan setiap ayat. **1 ayat/slide** memberi tepat satu ayat per slide. Pasal baru selalu dimulai di slide baru. Baris dipotong supaya teks sebesar mungkin di layar 16:9.

**Menyimpan ke susunan.** Ayat yang dilihat atau ditayangkan dari tab Alkitab tidak masuk pustaka. Tekan **+ Susunan** untuk menyimpannya ke pustaka profil sebagai materi Ayat, misalnya "Yohanes 3:16-18 (TB)", sekaligus menambahkannya ke susunan ibadah. **Edit materi** juga menyimpannya ke pustaka dulu; setelah itu teksnya bisa diubah seperti materi lain. Ayat yang sama tidak disimpan dua kali.

**Menghapus terjemahan.** Ikon tempat sampah di samping pilihan terjemahan menghapus terjemahan itu dari komputer setelah konfirmasi. Materi ayat yang sudah disimpan di pustaka tidak ikut terhapus.

**Format berkas Alkitab:**

- **OpenSong XML** (`<bible>` / `<b n="…">` / `<c n="…">` / `<v n="…">`): kitab dikenali dari namanya, dalam bahasa Indonesia atau Inggris. Bila namanya tidak dikenal tetapi berkas berisi tepat 66 kitab, urutannya yang dipakai.
- **Zefania XML** (`XMLBIBLE` / `BIBLEBOOK bnumber` / `CHAPTER` / `VERS`): kitab dikenali dari nomornya. Nama dan singkatan diambil dari bagian `INFORMATION` bila ada. Catatan kaki dan rujukan silang dibuang.
- **TSV UTF-8**, satu ayat per baris: `kitab[TAB]pasal[TAB]ayat[TAB]teks`, dengan kitab berupa nomor 1–66 atau nama / singkatan; atau `referensi[TAB]teks`, dengan referensi satu ayat seperti `Yoh 3:16`. Baris judul kolom di awal, baris kosong, dan baris yang diawali `#` dilewati.
- **EasyWorship `.ewb`**, dari EasyWorship 2009 sampai 8. Berkasnya ada di `C:\ProgramData\Softouch\EasyWorship.v8\Plugins\Bibles` (EasyWorship 8; `EasyWorship.v7` untuk versi 7, `EasyWorship` untuk 2009). Folder `ProgramData` tersembunyi, jadi Lentera membuka pemilih berkas langsung di sana bila ada. Nama dan singkatan terjemahan diambil dari berkas EasyWorship 6–8; untuk 2009 isi sendiri. Berkas hanya dibaca, tidak diubah.

Ayat gabungan seperti `5-6` didukung. Kitab di luar 66 kitab (misalnya Deuterokanonika), nomor ayat ganda, dan ayat kosong dilewati, dan jumlahnya disebutkan sebelum menyimpan. Bila ada baris TSV yang salah, Lentera menyebut nomor barisnya dan tidak menyimpan apa pun. Batas ukuran berkas 32 MB.

## Impor lirik dan ayat

Gunakan menu **Impor** di bilah atas. Untuk satu terjemahan Alkitab lengkap, pakai **Impor → Alkitab lengkap** (lihat **Alkitab**); impor di bawah ini menambahkan materi ke pustaka profil.

**Lirik lagu (.txt):** berkas UTF-8; nama berkas menjadi judul. Pisahkan bagian lirik dengan baris kosong. Tersedia `contoh-lirik.txt`.

**Lagu dari EasyWorship:** mengimpor semua lagu EasyWorship 2009, 6, 7, atau 8 sekaligus, langsung dari databasenya.

1. Pilih **Impor → Lagu dari EasyWorship**. Pemilih berkas terbuka di folder database EasyWorship di komputer ini bila ada:
   - EasyWorship 6–8: `C:\Users\Public\Documents\Softouch\EasyWorship\Default\v6.1\Databases\Data\Songs.db` (`SongWords.db` harus ada di folder yang sama).
   - EasyWorship 2009: `C:\Users\Public\Documents\Softouch\EasyWorship\Default\Databases\Data\Songs.DB` (bersama `Songs.MB`).

   Untuk mengimpor dari komputer lain, salin seluruh folder `Data` itu (misalnya ke flashdisk), lalu pilih `Songs.db` di salinannya.
2. Lentera menyebut berapa lagu yang ditemukan, berapa yang baru, berapa yang sudah ada, dan berapa yang judulnya diberi nomor. Tekan **Yes** untuk mengimpor.

Lagu yang judul dan liriknya sudah ada di pustaka profil dilewati; tanda baca, spasi, huruf besar/kecil, dan nomor seperti "(2)" di belakang judul tidak dihitung. Impor yang sama boleh diulang tanpa membuat lagu ganda. Lagu dengan judul yang sudah dipakai tetapi liriknya berbeda diberi nomor: "Kasih (2)", "Kasih (3)". Bila punya beberapa versi EasyWorship, impor dari versi terbaru dulu supaya versi lirik lama yang mendapat nomor.

Pembagian slide dari EasyWorship dipertahankan. Label bagian seperti "Verse 1", "Chorus", "Reff", dan "End" dibuang; baris lirik lain tetap ikut. Lagu tanpa lirik dilewati. Huruf tebal, warna, dan pengaturan tampilan EasyWorship tidak ikut. Berkas EasyWorship hanya dibaca dan tidak pernah diubah; EasyWorship boleh tetap terbuka.

**Ayat Alkitab (.tsv):** satu ayat per baris, dengan format `referensi[TAB]teks`. `[TAB]` berarti karakter tab, bukan tulisan literal. Tersedia `contoh-ayat.tsv`. Jika perlu baris baru di dalam satu ayat, tulis `\n` pada kolom teks. Baris yang diawali `#` diabaikan. Duplikat referensi + isi dilewati. Seluruh berkas diperiksa sebelum data ditambahkan. Batas ukuran impor adalah 32 MB.

Pustaka menampilkan semua materi, urut menurut waktu ditambahkan. Ketik judul, referensi, atau kata di kolom pencarian untuk mempersempit hasil. Aplikasi ini tidak menyertakan teks Alkitab atau database lagu berlisensi. Gunakan materi yang Anda berhak gunakan.

## Profil

Setiap profil punya pustaka lirik, ayat, dan susunan ibadahnya sendiri, misalnya satu profil untuk ibadah umum dan satu untuk komisi pemuda. Tekan tombol profil di kiri atas untuk membuka layar **Pilih profil**:

- Klik kartu profil untuk membukanya. Kartu berbingkai kuning adalah profil yang sedang dibuka. Tekan **Esc** atau **×** untuk kembali tanpa berganti.
- **Profil baru** membuat profil dengan pustaka kosong lalu langsung membukanya. Latar umum disalin dari profil yang sedang dibuka.
- Ikon pensil mengganti nama profil. Nama wajib diisi, paling panjang 40 karakter, dan tidak boleh sama dengan profil lain.
- Ikon tempat sampah menghapus profil lain (bukan yang sedang dibuka). Folder profil dipindahkan ke `profiles-deleted` di folder data, jadi masih bisa dipulihkan: tutup Lentera lalu pindahkan foldernya kembali ke `profiles`.

Lentera membuka profil yang terakhir dipakai. Bila ada lebih dari satu profil, layar **Pilih profil** muncul saat aplikasi dibuka. Pilihan monitor proyektor mengikuti komputer, jadi tidak berubah saat berganti profil. Bila ada slide yang sedang tayang, Lentera meminta konfirmasi dulu karena berganti profil menghentikan tayangan.

Nama profil yang sedang dibuka juga tampil di judul jendela. Pada jendela yang lebih sempit dari 1200 px, tombol profil hanya menampilkan lingkarannya.

## Data dan cadangan

Folder data ada di `%LOCALAPPDATA%\Lentera` (macOS: `~/Library/Application Support/Lentera`). Data disimpan per profil di `profiles\<id profil>\library.db`, sebuah basis data SQLite, termasuk susunan ibadah, pilihan monitor, dan latar. Gambar latar profil ada di `profiles\<id profil>\backgrounds`. Saat Lentera versi ini pertama kali membuka profil dari versi 0.4.0 atau sebelumnya, `library.db` diperbarui otomatis untuk latar per materi; setelah itu profil tidak bisa dibuka lagi oleh versi lama. Profil yang dihapus dipindahkan ke `profiles-deleted\<id profil>`.

Terjemahan Alkitab yang diimpor disimpan di `alkitab.db` di folder data, satu untuk semua profil di komputer ini, dan tidak ikut terhapus bersama profil. Berkas ini tidak punya cadangan `.bak`; simpan berkas aslinya supaya bisa diimpor ulang.

Saat Lentera 0.3.0 atau yang lebih baru pertama kali dibuka, isi `library.json` dari Lentera 0.2.1 atau sebelumnya dipindahkan otomatis ke Profil utama. `library.json` tidak diubah dan tidak dibaca lagi, sehingga tetap ada sebagai cadangan. Bila `library.json` rusak, Lentera berhenti dengan pesan dan tidak membuat profil.

Setiap penyimpanan berlangsung dalam satu transaksi, jadi listrik padam saat menyimpan tidak meninggalkan data setengah tersimpan. `library.db.bak` berisi data profil saat Lentera terakhir dibuka; untuk memulihkannya, tutup Lentera lalu ganti `library.db` dengan salinan `library.db.bak`. Cadangan ini bukan riwayat lengkap; salin folder data secara berkala, terutama sebelum impor besar. Aplikasi tidak mengganti data rusak dengan pustaka kosong. Hanya satu proses dapat membuka folder data yang sama.

Untuk menyimpan di lokasi lain, jalankan `lentera.exe --data-dir "D:\Data Ibadah"`. Setelah pembaruan lewat tombol **Perbarui**, Lentera dibuka kembali dengan folder data yang sama. Tutup aplikasi sebelum menyalin atau memulihkan cadangan. Layar tayang menyimpan salinan slide sehingga mengedit materi tidak langsung mengubah slide yang sedang ditampilkan.

## Ruang lingkup versi ini

- Lirik, ayat teks, tab Alkitab dengan referensi dan terjemahan yang diimpor, pencarian, editor, susunan ibadah, pratinjau, keluaran terpisah, layar penuh, blackout, dan latar berupa warna atau gambar, untuk semua materi atau per materi.
- Windows 10/11 **64-bit** dengan installer dan pembaruan otomatis. macOS dari kode sumber. Antarmuka ditujukan untuk layar minimal sekitar 1024×640.
- Impor lagu dan Alkitab EasyWorship 2009–8. Jadwal, tema, dan media EasyWorship belum ikut; gambar latar EasyWorship bisa ditambahkan satu per satu atau beberapa sekaligus lewat galeri latar.
- Belum mencakup video, PowerPoint, NDI, streaming, atau pencarian kata di seluruh Alkitab.
- Belum ada sertifikat penandatanganan aplikasi. Jangan gunakan untuk layanan langsung sebelum latihan dan pengujian di komputer / proyektor tujuan.

## Untuk pengembang

Logika data, susunan ibadah, impor, dan penyimpanan ada di `crates/lentera-core` beserta pengujiannya. Penyimpanan SQLite per profil dan pemindahan `library.json` ada di `src/store.rs`; skemanya diberi versi lewat `PRAGMA user_version`. Alkitab ada di `src/bible/`: daftar 66 kitab, pembaca referensi, impor OpenSong / Zefania / TSV, pembagian slide ayat, dan `alkitab.db`. Impor EasyWorship ada di `src/easyworship/`: lagu SQLite + RTF (6–8), tabel Paradox (2009), dan Alkitab `.ewb`; semuanya hanya membaca. Aplikasi ada di `crates/lentera-app`: logika di `src/main.rs`, tata letak dan tema di `ui/app.slint`, pembaruan otomatis (Velopack) di `src/update.rs`. Tampilan digambar dengan renderer software Slint, tanpa browser tertanam atau server lokal. Tampilan Win32 lama ada di `crates/lentera-win32` dan tidak dirilis lagi. Rilis Windows dibuat oleh workflow `.github/workflows/rilis.yml` saat tag versi di-push. CI (`inti.yml`) berjalan untuk setiap PR dan push ke `main`; build ujinya memakai profil `ci` (release tanpa LTO) dan cache Rust yang diisi dari `main`. Rilis selalu dibangun bersih dengan profil `release`. Server login dan sinkron (Go + PostgreSQL, belum dipakai aplikasi) ada di `server/`; protokolnya di `docs/SINKRON.md`.

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

Logo ada di `crates/lentera-app/assets/icon/`: `lentera.svg` untuk ukuran 40 px ke atas dan di dalam aplikasi, `lentera-kecil.svg` (garis lebih tebal) untuk 16–32 px. Setelah mengubah salah satunya, buat ulang `lentera.ico` dengan `python scripts/buat_ikon.py` (butuh Pillow dan Chrome atau Edge) lalu commit hasilnya. `build.rs` menanam ikon itu dan info versi ke `lentera.exe` tanpa `rc.exe`; workflow rilis memakainya juga untuk `Setup.exe`.

Kode sumber memakai lisensi MIT. Tampilan memakai Slint dengan lisensi Slint Royalty-free 2.0, yang mewajibkan atribusi di layar **Tentang**; jangan menghapus widget `AboutSlint` di `ui/app.slint`. Teks contoh adalah placeholder, bukan kumpulan lagu atau Alkitab berlisensi.
