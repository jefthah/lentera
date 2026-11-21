# Riwayat rilis

## Belum dirilis

**Alkitab:** impor satu terjemahan sekali, lalu cukup ketik referensi seperti `Yoh 3:16-18` dan tekan Enter untuk menayangkannya.

- **Tab Alkitab** di kolom kiri, di samping Pustaka. Pilih terjemahan lalu ketik referensi; slide ayatnya langsung tampil di tengah untuk dipratinjau. **Enter** menayangkan slide pertama.
- Referensi yang dikenali: `Yoh 3:16`, `Yoh 3:16-18`, `Yoh 3.16`, `Yoh 3:16-4:2`, `Rm 8:28, 31-39`, `Yoh 3:16; 4:1`, satu pasal (`Mzm 23`), beberapa pasal (`Mzm 1-2`), dan `Yud 3` untuk kitab yang hanya satu pasal. Nama kitab boleh ditulis lengkap, dengan singkatan TB (`Kej`, `Mzm`, `1Kor`, `Why`), dengan angka Romawi (`II Kor`), atau dalam bahasa Inggris (`John`, `Ps`). Bila nama kitab belum jelas (misalnya `Ye`), Lentera menyebut pilihannya. Rentang yang melewati ayat terakhir dipendekkan (`Yoh 3:16-99` menjadi `Yoh 3:16-36`).
- Daftar ayat satu pasal di bawah kotak referensi: klik ayat untuk memilihnya, **Shift+klik** untuk memilih rentang, klik dua kali untuk langsung menayangkan.
- Slide dibagi otomatis 1–3 ayat menurut panjangnya, dengan nomor ayat kecil di depan setiap ayat. Pilihan **1 ayat/slide** juga tersedia. Baris dipotong supaya teks sebesar mungkin di layar 16:9.
- Ayat yang dilihat atau ditayangkan tidak masuk pustaka. **+ Susunan** menyimpannya ke pustaka profil sebagai materi Ayat, misalnya "Yohanes 3:16-18 (TB)", lalu menambahkannya ke susunan ibadah. **Edit materi** juga menyimpannya ke pustaka dulu. Ayat yang sama tidak disimpan dua kali.
- **Impor Alkitab** lewat tombol di tab Alkitab atau menu **Impor → Alkitab lengkap**: OpenSong XML, Zefania XML, atau TSV UTF-8 (`kitab [TAB] pasal [TAB] ayat [TAB] teks`, atau `referensi [TAB] teks`). Ayat gabungan seperti `5-6` didukung. Kitab di luar 66 kitab, nomor ganda, dan ayat kosong dilewati dan jumlahnya disebutkan. Singkatan (misalnya TB) dan nama terjemahan diisi sebelum disimpan.
- Terjemahan disimpan di `alkitab.db` di folder data, berlaku untuk semua profil di komputer ini, dan tidak ikut sinkron. Tombol tempat sampah menghapusnya lagi.
- Lentera tetap tidak menyertakan teks Alkitab. Gunakan terjemahan yang Anda berhak pakai.

**Akun dan profil:** satu komputer kini bisa memakai beberapa profil, masing-masing dengan pustaka lirik, ayat, dan susunan ibadahnya sendiri. Data tersimpan per profil dalam basis data SQLite.

- **Pilih profil.** Tombol profil (lingkaran berhuruf awal dan nama profil) menggantikan tulisan "Lentera" di bilah atas dan membuka layar **Pilih profil**. Di sana profil bisa dibuka, dibuat (**Profil baru**), diganti namanya, dan dihapus. Nama profil juga tampil di judul jendela. Pada jendela yang lebih sempit dari 1200 px, tombol profil hanya menampilkan lingkarannya supaya tombol proyektor dan impor tidak terpotong.
- Lentera membuka profil yang terakhir dipakai. Bila ada lebih dari satu profil, layar **Pilih profil** langsung muncul saat aplikasi dibuka.
- Profil baru mulai dengan pustaka kosong; latar layar disalin dari profil yang sedang dibuka. Pilihan monitor proyektor mengikuti komputer, bukan profil, jadi tidak berubah saat berganti profil.
- Nama profil wajib diisi, paling panjang 40 karakter, dan tidak boleh sama dengan profil lain (huruf besar/kecil dianggap sama).
- Berganti profil saat ada slide yang tayang meminta konfirmasi, lalu menghentikan tayangan. Perubahan di editor disimpan dulu.
- **Hapus profil** tidak menghapus berkas: folder profil dipindahkan ke `profiles-deleted` di folder data sehingga masih bisa dipulihkan secara manual. Profil yang sedang dibuka dan profil terakhir tidak bisa dihapus.
- **Data lama pindah otomatis.** Saat pertama kali dibuka, isi `library.json` dipindahkan ke profil pertama, **Profil utama**, termasuk susunan ibadah, latar, dan pilihan monitor. `library.json` tidak diubah dan tetap ada sebagai cadangan. Bila berkas itu rusak, Lentera berhenti dengan pesan tanpa membuat profil.
- Data baru berada di `profiles/<id profil>/library.db` di folder data. Setiap materi kini punya ID unik (UUID) serta waktu dibuat, diubah, dan dihapus, sebagai dasar sinkron antar perangkat nanti. Materi yang dihapus disimpan sebagai tanda hapus tanpa judul dan isi.
- Penyimpanan hanya menulis materi yang berubah, dalam satu transaksi, jadi listrik padam saat menyimpan tidak meninggalkan data setengah tersimpan.
- `library.db.bak` berisi data saat Lentera terakhir dibuka (sebelumnya `library.json.bak` berisi versi sebelum penyimpanan terakhir).
- Program bertambah sekitar 1,1 MB karena SQLite ikut dibangun ke dalamnya. Pemakaian RAM tidak terlihat bertambah.

## 0.2.1 — 18 November 2026 — beta

Lentera kini tampil dengan logonya sendiri di Windows.

- **Logo baru: lentera menyala.** Tampil di title bar, taskbar, shortcut Desktop dan Start Menu, installer, serta pojok kiri atas dan layar **Tentang** aplikasi. Sebelumnya Windows menampilkan ikon aplikasi generik.
- `lentera.exe` kini membawa info versi (nama, deskripsi, versi, hak cipta) yang terlihat di Properties dan Task Manager.
- Fitur, data, dan tombol cepat sama dengan 0.2.0. Pengguna 0.1.1 dan 0.2.0 menerima versi ini lewat pembaruan otomatis. Bila shortcut di Desktop masih menampilkan ikon lama, Windows masih memakai cache ikonnya; ikon baru muncul setelah keluar-masuk akun atau restart.

## 0.2.0 — 17 November 2026 — beta

Tampilan baru untuk Windows dan macOS, dibangun dengan Slint.

- **Tampilan baru bertema gelap.** Susunan ibadah dan pustaka di kiri, semua slide materi sebagai kartu di tengah, lalu pratinjau, tombol **Tayangkan**, layar yang sedang tayang, dan kontrol di kanan. Slide yang dipratinjau berbingkai kuning, yang sedang tayang berbingkai merah.
- **Editor materi** dibuka lewat **Edit materi** atau **Materi baru**, dengan hasil slide yang langsung terlihat saat mengetik. Bilah atas menunjukkan apa yang sedang tayang selama mengedit.
- Klik dua kali slide, materi, atau baris susunan untuk langsung menayangkan. Pilihan monitor proyektor dan menu **Impor** pindah ke bilah atas.
- Layar **Tentang** (tombol ⓘ) menampilkan versi dan atribusi Slint.
- Fitur, tombol cepat, folder data, dan format `library.json` sama dengan 0.1.1.
- Pengguna 0.1.1 menerima versi ini lewat pembaruan otomatis. Nama program tetap `lentera.exe`, jadi shortcut lama tetap jalan.
- Program menjadi sekitar 11 MB (sebelumnya 2,2 MB) dan memakai sekitar 34 MiB RAM pada data contoh (sebelumnya sekitar 15 MiB).
- Aplikasi juga dibangun dan diuji otomatis di macOS. Installer macOS belum tersedia.

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
