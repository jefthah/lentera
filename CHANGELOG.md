# Riwayat rilis

## Belum dirilis

**Latar video.** Video bisa dipakai sebagai latar umum atau latar per materi, sama seperti gambar. Untuk sekarang hanya di Windows.

- **Tambah gambar atau video** di galeri latar menerima video MP4, M4V, MOV, dan WMV sampai 500 MB, satu per satu atau beberapa sekaligus. Bila EasyWorship terpasang, pemilih berkas terbuka di folder gambar atau video EasyWorship.
- **Di proyektor** video diputar berulang tanpa suara, dengan lirik di atasnya seperti di atas gambar.
  - Video tetap berjalan saat slide berganti, selama latarnya sama.
  - Video berhenti saat proyektor ditutup atau digelapkan, lalu mulai lagi dari awal.
- **Layar jemaat** di kolom kanan ikut bergerak, dengan sepertiga frame supaya ringan. Slide, pratinjau, dan galeri memakai gambar diam dari detik pertama video, dan galeri memberi tanda **Video**.
- **Ringan:** video didecode di GPU lewat Media Foundation bawaan Windows, tanpa codec tambahan. Bila GPU tidak bisa dipakai, video didecode CPU. Video yang lebih besar dari 1920×1080 diperkecil saat diputar.
- **Bila video tidak bisa diputar**, Lentera memberi pesan dan proyektor memakai gambar diamnya.
- Video disalin apa adanya ke folder profil, tanpa diperkecil. Seperti gambar latar, video tetap per komputer dan tidak disinkron.
- Di macOS tombolnya tetap **Tambah gambar**; video belum bisa diputar di sana.

**Kebutuhan:** dengan video 1080p, Lentera memakai sekitar 200 MiB RAM (tanpa video sekitar 40 MiB) dan sekitar sepertiga inti CPU. Video 4K butuh sekitar 330 MiB. Di komputer yang tidak bisa decode di GPU, video 1080p butuh sekitar 310 MiB dan hampir satu inti, sedangkan 4K sekitar 850 MiB dan lebih dari dua inti. Pakai video 1080p.

**Membuka profil dengan versi lama:** format profil tidak berubah. Lentera 0.7.0 atau sebelumnya tidak menampilkan video di galeri, dan materi atau latar umum yang memakai video kembali ke latar umum atau warna. Berkas videonya tetap ada di profil. Program kini sekitar 14,3 MB (0.7.0: 14,2 MB), tanpa komponen pihak ketiga baru.

## 0.7.0 — 25 November 2026 — beta

**Akun dan sinkron.** Profil yang sama bisa dipakai di beberapa komputer, misalnya laptop di rumah dan komputer gereja.

- **Masuk dengan kode email.** Jendela **Masuk ke Lentera** muncul saat pertama kali dibuka: email, lalu kode 6 digit. Tidak ada kata sandi. Setelah masuk sekali, Lentera bisa dipakai tanpa internet.
- **Yang disinkron:** lirik, ayat, susunan ibadah, nama profil, dan warna latar umum.
  - Sinkron berjalan saat dibuka, setiap 30 detik, setelah berganti profil, setelah mengganti nama atau menghapus profil, dan lewat **Sinkron sekarang**.
  - Pilihan monitor, gambar latar, latar per materi, dan Alkitab tetap per komputer.
- **Data yang sudah ada:** profil di komputer ditambahkan ke akun. Komputer tanpa data mengunduh profil akun dulu, jadi tidak ada Profil utama ganda.
- **Tombol akun di bilah atas** menunjukkan Tersinkron, Offline, Masuk lagi, atau Perlu perhatian. Menunya berisi email, keterangan, **Sinkron sekarang**, dan **Keluar dari akun**.
  - **Keluar dari akun** menutup Lentera tanpa menghapus data.
  - Jam komputer yang berbeda lebih dari 5 menit dari server diperingatkan.
- **Aman saat server mati.** Bila server tidak terjangkau dan komputer sudah berisi profil, **Buka tanpa masuk untuk sementara** membuka data lokal. Sesi yang berakhir tidak mengunci aplikasi; Lentera hanya meminta **Masuk lagi**.
- **Profil yang dihapus** di komputer lain tetap tersimpan di komputer ini, tetapi tidak disinkron lagi dan ditandai **Perlu perhatian**.
- Token sesi disimpan di Windows Credential Manager, bukan di folder data.

**Perbaikan:**

- **Lagu dari EasyWorship 7 tidak lagi menghentikan sinkron.**
  - Sebagian lirik EasyWorship 7 berisi karakter NUL tak terlihat di ujung baris. Server menolak karakter itu, sehingga satu lagu saja membuat seluruh kiriman gagal, dengan pesan keliru "Isi materi paling besar 1 MB".
  - Kini impor EasyWorship, impor berkas, dan editor membuang karakter kontrol seperti itu. Baris baru dan tab tetap ada.
  - Lagu yang sudah diimpor Lentera 0.6 dibersihkan saat profilnya dibuka, dan sebelum dikirim ke server.
- **Kotak konfirmasi "Keluar dari akun" kini muncul.**
  - Sebelumnya kotak itu tersembunyi, sehingga jendela Lentera terkunci dan tampak macet.
  - Semua kotak pesan Lentera kini dijaga supaya selalu tampil.

**Memperbarui:** pengguna 0.1.1 sampai 0.6.0 menerima versi ini lewat pembaruan otomatis; pengguna 0.1.0 memasang installer sekali. Setelah pembaruan, Lentera meminta masuk saat pertama kali dibuka. Langkah ini butuh internet dan email, jadi lakukan di luar ibadah. Versi ini tidak mengubah format profil: sinkron hanya menambah tabel `synced` dan beberapa kunci di tabel `meta`, sehingga profil yang sudah dibuka 0.7.0 tetap bisa dibuka lagi oleh 0.6.0. Bila memperbarui langsung dari 0.4.0 atau sebelumnya, profil diperbarui untuk latar per materi seperti di 0.5.0, dan setelah itu tidak bisa dibuka lagi oleh 0.4.0 atau sebelumnya. Program kini sekitar 14,2 MB (0.6.0: 13,9 MB). Komponen pihak ketiga baru: `keyring` untuk menyimpan token di Windows Credential Manager, beserta `byteorder` dan `windows-sys` 0.60 yang dipakainya.

## 0.6.0 — 23 November 2026 — beta

Layar operator memakai desain baru. Yang dilihat jemaat kini ada di atas kolom kanan, susunan ibadah menunjukkan sampai mana ibadah berjalan, editor tidak lagi menutup kontrol tayang, Alkitab dibuka di kolom tengah, dan layar proyektor menyebut referensi ayat yang tayang.

**Layar operator baru:**

- **Bilah atas:** logo Lentera, tombol profil, kotak cari di tengah (**Ctrl+K**), tombol proyektor, **Impor**, dan **ⓘ**.
  - Saat proyektor terbuka, tombolnya hijau dengan tulisan **Proyektor menyala** dan nama monitornya. Saat tertutup, klik **Buka proyektor** untuk membukanya.
  - Panah di tombol itu membuka menu proyektor: pilihan monitor, latar umum, dan **Buka / Tutup proyektor**. Kotak pilihan monitor di bilah atas diganti menu ini.
- **Susunan ibadah:**
  - Materi yang sedang tayang bertanda **Tayang**, dengan keterangan seperti "Lirik · slide 3 dari 8". Materi sebelumnya bertanda centang.
  - Tombol naik, turun, dan lepas muncul saat baris disorot, dan di baris terpilih yang tidak sedang tayang.
- **Pustaka dan Alkitab:**
  - Keduanya kini tab bersegmen, dan jumlah materi pustaka tampil di tab.
  - Materi yang sudah ada di susunan bertanda **Di susunan**. Tombol **+ Susunan** muncul saat baris disorot atau dipilih.
  - **Materi baru** menjadi **Baru**.
- **Tengah:**
  - Di atas judul ada label **Lirik** atau **Ayat**, jumlah slide, dan urutan materi di susunan, misalnya "materi ke-2 di susunan".
  - **Edit materi** menjadi **Edit lirik** atau **Edit ayat**.
  - Petunjuk di bawah slide: klik untuk pratinjau, klik dua kali untuk langsung tayang, tombol 1–9 untuk lompat ke slide.
- **Kanan:**
  - **Layar jemaat** (dulu **Tayang**) kini di atas. Di bawahnya tampil judul, jenis, dan latar materi yang tayang.
  - Lalu **Sebelumnya / Berikutnya**, pratinjau kecil, dan **Tayangkan**. Di samping pratinjau ada keterangan apakah slide itu sudah dilihat jemaat.
  - **Gelapkan**, **Layar penuh**, dan **Latar umum** ada di bagian bawah.
- **Tombol cepat baru:**
  - **Ctrl+K** pindah ke kotak cari.
  - **1–9** langsung menayangkan slide itu dari materi terpilih, saat tidak sedang mengetik.
  - **Esc** di kotak cari atau kotak teks lain kini hanya keluar dari kotak itu. Layar penuh dan proyektor tidak berubah.
- **Editor di kolom tengah:**
  - Kolom kanan dengan layar jemaat, **Sebelumnya / Berikutnya**, **Tayangkan**, dan **Gelapkan** tetap terlihat saat mengedit. Lirik bisa dibetulkan saat ibadah berjalan.
  - Bilah atas tetap sama saat mengedit, termasuk tombol proyektor.
  - Di atas editor: **Kembali ke slide** (dulu **Kembali ke tayang**, di bilah atas), tanda **Tersimpan** / **Menyimpan…**, dan **Hapus** (dulu **Hapus materi**).
  - Formulir di kiri: **Judul**, **Jenis** (**Lirik lagu** amber atau **Ayat Alkitab** biru), dan **Isi**.
  - **Hasil slide** di kanannya, satu kolom bernomor. Di bawahnya ada keterangan berapa slide lagi di bawah, lalu **Tambah ke susunan**.
- **Alkitab di kolom tengah:**
  - Tab **Alkitab** membuka Alkitab di kolom tengah. Kotak referensi besar di atas langsung siap diketik, dan **Enter** menayangkan. Di bawahnya ada contoh penulisan referensi, atau pesan bila referensi tidak dikenal.
  - Judul referensi tampil besar dengan terjemahan, jumlah ayat dan slide, dan **belum di pustaka**, **ada di pustaka**, atau **ada di susunan**. Di sampingnya ada **Otomatis / 1 ayat/slide** dan **Ke susunan** (dulu **Susunan**).
  - Daftar ayat satu pasal kini menampilkan teks ayat utuh. Tombol **Pasal** sebelumnya dan berikutnya di atas daftar membuka ayat 1 pasal itu. Klik, Shift+klik, dan klik dua kali tetap sama.
  - **Hasil slide** ayat yang dipilih tampil di kanan daftar: klik untuk pratinjau, klik dua kali untuk tayang.
  - Memilih materi dari susunan atau pustaka mengembalikan kolom tengah ke slide materi itu. Klik tab **Alkitab** lagi untuk kembali ke ayat terakhir.
- **Tab Alkitab di kiri:**
  - Pilihan terjemahan dan tombol hapus terjemahan tetap di atas.
  - **Terakhir dibuka** berisi sampai 8 referensi yang terakhir ditayangkan atau ditambahkan ke susunan, dengan jumlah ayatnya. Klik untuk membukanya lagi. Daftar ini disimpan di `alkitab.db`, jadi sama untuk semua profil di komputer ini.
  - **Impor terjemahan** ada di bawah, dengan jumlah terjemahan yang terpasang. Tombol ini menggantikan ikon impor di samping pilihan terjemahan.
  - Saat tab Alkitab dibuka, bilah bawah menyebut terjemahan yang siap, misalnya "Terjemahan Baru siap · 31.102 ayat".
- **Referensi dari kotak cari:**
  - Ketik referensi seperti `yoh 3:16` di kotak cari (**Ctrl+K**). Di atas daftar pustaka muncul **Buka Yohanes 3:16**. Tekan **Enter** atau klik baris itu untuk membukanya di Alkitab.
  - Mengetik di kotak cari saat tab Alkitab atau Susunan terbuka kini pindah ke tab Pustaka, supaya hasilnya terlihat.
- **Referensi ayat di layar proyektor:**
  - Di bawah ayat yang tayang tampil referensinya, misalnya "Yohanes 3:16 (TB)". Referensi ini mengikuti ayat di setiap slide, jadi slide "Yohanes 3:16-18" yang hanya berisi ayat 17 bertuliskan "Yohanes 3:17 (TB)".
  - Referensi hanya muncul bila judul materi Ayat berupa referensi, misalnya hasil **Ke susunan** atau ayat TSV `Yoh 3:16`. Materi Ayat dengan judul lain dan lagu tidak berubah.
  - **Layar jemaat** di kolom kanan ikut menampilkannya. Kartu slide di tengah dan Hasil slide tidak.
  - Teks ayat memakai huruf Semibold, sedikit lebih tipis dari lirik, seperti di desain.
- **Jendela sempit** (di bawah 1200 px, misalnya laptop 1024×640):
  - Kolom kiri dan kanan lebih sempit.
  - Susunan ibadah pindah ke tab **Susunan** di samping Pustaka dan Alkitab.
  - Latar umum hanya ada di menu proyektor.

Fungsi lain tidak berubah.

**Memperbarui:** pengguna 0.1.1 sampai 0.5.0 menerima versi ini lewat pembaruan otomatis; pengguna 0.1.0 memasang installer sekali. Versi ini tidak mengubah format data. Profil yang sudah dibuka 0.5.0 tetap bisa dibuka lagi oleh 0.5.0. Daftar **Terakhir dibuka** memakai tabel yang sudah ada di `alkitab.db`, jadi Alkitab yang sudah diimpor tetap bisa dibuka oleh versi sebelumnya. Bila memperbarui langsung dari 0.4.0 atau sebelumnya, profil diperbarui untuk latar per materi seperti di 0.5.0, dan setelah itu tidak bisa dibuka lagi oleh 0.4.0 atau sebelumnya. Program kini sekitar 13,9 MB (0.5.0: 13,3 MB). Tidak ada komponen pihak ketiga baru.

## 0.5.0 — 22 November 2026 — beta

Lirik dan ayat kini bisa tampil di atas gambar latar, untuk semua materi sekaligus atau per lagu dan ayat.

**Latar gambar:** layar jemaat kini bisa memakai gambar sebagai latar, tidak hanya warna. Satu gambar bisa menjadi latar umum, dan setiap lagu atau ayat bisa diberi gambar sendiri.

- **Latar umum** di kolom kanan (dulu **Latar layar**) berisi tiga warna dan kotak **Gambar**. Kotak Gambar membuka galeri gambar profil. **Tambah gambar** memilih satu atau beberapa gambar JPG, JPEG, JFIF, atau PNG sekaligus. Bila komputer ini punya folder gambar EasyWorship, pemilih berkas terbuka di sana. Klik gambar untuk memakainya sebagai latar umum, atau klik salah satu warna untuk kembali ke latar warna.
- **Latar per materi:** di atas judul materi terpilih ada tombol **Latar umum**, atau **Latar: nama gambar** bila materi itu punya gambar sendiri. Tombol ini membuka galeri untuk materi itu saja. Klik gambar untuk memakainya, atau **Pakai latar umum** untuk kembali. Ayat dari tab Alkitab disimpan dulu ke pustaka, seperti saat diedit.
- Slide di tengah, pratinjau, dan hasil editor memakai latar materi yang dipilih. Layar Tayang dan proyektor memakai latar materi yang sedang tayang, jadi latarnya ikut berganti saat tayangan pindah ke materi berikutnya dalam susunan. Mengganti latar umum tidak mengubah materi yang punya gambar sendiri.
- Di atas gambar, lirik dan ayat tampil putih dengan garis tepi gelap, dan gambarnya sedikit digelapkan, supaya teks tetap terbaca juga di gambar yang terang.
- Gambar disalin ke folder profil sebagai JPG dan diperkecil seperlunya untuk layar 1920×1080 (tidak pernah diperbesar). Foto kamera ditampilkan tegak sesuai arah pengambilannya. Berkas aslinya tidak diubah, dan gambar yang sama tidak disimpan dua kali.
- Gambar mengisi seluruh layar; bagian yang tidak muat dipotong rata tengah. Di proyektor tegak (potret), sisi kiri dan kanan gambar 16:9 banyak terpotong.
- Tombol tempat sampah di galeri menghapus gambar dari profil setelah konfirmasi. Materi yang memakai gambar itu kembali ke latar umum, dan bila gambar itu latar umum, latar umum kembali ke warna. Berkas aslinya di komputer tidak ikut terhapus.
- Setiap profil punya galeri sendiri. **Profil baru** ikut memakai gambar latar umum profil yang sedang dibuka.
- Belum tersedia: video sebagai latar.

**Memperbarui:** pengguna 0.1.1 sampai 0.4.0 menerima versi ini lewat pembaruan otomatis; pengguna 0.1.0 memasang installer sekali. Saat versi ini pertama kali membuka sebuah profil, `library.db` diperbarui otomatis untuk menyimpan latar per materi. Isi pustaka dan susunan ibadah tetap, tetapi setelah itu profil tersebut tidak bisa dibuka lagi oleh Lentera 0.4.0 atau sebelumnya. Bila memperbarui dari 0.2.1 atau sebelumnya, pustaka lama pindah ke **Profil utama** saat versi ini pertama kali dibuka. Program kini sekitar 13,3 MB (0.4.0: 12,9 MB). Pengolah gambar JPEG dan PNG sudah ikut lewat Slint, jadi tidak ada komponen pihak ketiga baru.

## 0.4.0 — 22 November 2026 — beta

Lagu dan Alkitab dari EasyWorship bisa dipindahkan langsung, dan pustaka menampilkan semua materi.

**Impor dari EasyWorship:** lagu dan Alkitab dibaca langsung dari database EasyWorship, tanpa ekspor atau konversi dulu.

- **Impor → Lagu dari EasyWorship** mengimpor semua lagu sekaligus dari EasyWorship 2009, 6, 7, atau 8. Pemilih berkas terbuka di folder database EasyWorship di komputer ini; pilih `Songs.db`. Sebelum mengimpor, Lentera menyebut jumlah lagu yang ditemukan, yang baru, yang sudah ada, dan yang judulnya diberi nomor.
- Lagu yang judul dan liriknya sudah ada di pustaka dilewati (tanda baca, spasi, dan huruf besar/kecil diabaikan), jadi impor boleh diulang tanpa membuat lagu ganda. Lagu dengan judul yang sudah dipakai tetapi liriknya berbeda diberi nomor, misalnya "Kasih (2)".
- Pembagian slide dari EasyWorship dipertahankan. Label bagian seperti "Verse 1", "Chorus", "Reff", dan "End" dibuang; baris lirik lain tetap ikut. Lagu tanpa lirik dilewati.
- **Alkitab EasyWorship (`.ewb`)** dari EasyWorship 2009 sampai 8 bisa diimpor lewat **Impor → Alkitab lengkap**. Nama dan singkatan terjemahan diambil dari berkas bila ada. Pemilih berkas terbuka di folder Alkitab EasyWorship, yang biasanya tersembunyi.
- Berkas EasyWorship hanya dibaca dan tidak pernah diubah; EasyWorship boleh tetap terbuka.
- Pastikan gereja berhak memakai lagu dan terjemahan Alkitab itu di luar EasyWorship.

**Pustaka menampilkan semua materi.** Sebelumnya daftar pustaka berhenti di 500 materi pertama, jadi setelah mengimpor ratusan lagu sisanya hanya bisa ditemukan lewat pencarian. Kini semua materi tampil, urut menurut waktu ditambahkan, dan tetap ringan: 5.000 lagu dimuat dalam sekitar 4 ms di komputer pengembangan. Jumlah di atas daftar memakai titik ribuan, misalnya "5.000 materi".

**Perbaikan:** Lentera tidak lagi macet setelah memilih berkas impor. Di 0.3.0, pesan yang muncul tepat setelah dialog pilih berkas (misalnya "Impor gagal" karena berkas lirik bukan UTF-8) tidak pernah terlihat: jendela Lentera terkunci, dan aplikasi harus ditutup paksa. Kini pesan itu tampil di depan seperti biasa, untuk impor lirik, ayat, lagu EasyWorship, dan Alkitab.

**Memperbarui:** pengguna 0.1.1 sampai 0.3.0 menerima versi ini lewat pembaruan otomatis; pengguna 0.1.0 memasang installer sekali. Profil dan pustaka 0.3.0 dipakai apa adanya. Bila memperbarui dari 0.2.1 atau sebelumnya, pustaka lama pindah ke **Profil utama** saat versi ini pertama kali dibuka. Program kini sekitar 12,9 MB (0.3.0: 12,8 MB).

## 0.3.0 — 21 November 2026 — beta

Ayat Alkitab cukup diketik referensinya, dan satu komputer bisa memakai beberapa profil.

**Alkitab:** impor satu terjemahan sekali, lalu cukup ketik referensi seperti `Yoh 3:16-18` dan tekan Enter untuk menayangkannya.

- **Tab Alkitab** di kolom kiri, di samping Pustaka. Pilih terjemahan lalu ketik referensi; slide ayatnya langsung tampil di tengah untuk dipratinjau. **Enter** menayangkan slide pertama.
- Referensi yang dikenali: `Yoh 3:16`, `Yoh 3:16-18`, `Yoh 3.16`, `Yoh 3:16-4:2`, `Rm 8:28, 31-39`, `Yoh 3:16; 4:1`, satu pasal (`Mzm 23`), beberapa pasal (`Mzm 1-2`), dan `Yud 3` untuk kitab yang hanya satu pasal. Nama kitab boleh ditulis lengkap, dengan singkatan TB (`Kej`, `Mzm`, `1Kor`, `Why`), dengan angka Romawi (`II Kor`), atau dalam bahasa Inggris (`John`, `Ps`). Bila nama kitab belum jelas (misalnya `Ye`), Lentera menyebut pilihannya. Rentang yang melewati ayat terakhir dipendekkan (`Yoh 3:16-99` menjadi `Yoh 3:16-36`).
- Daftar ayat satu pasal di bawah kotak referensi: klik ayat untuk memilihnya, **Shift+klik** untuk memilih rentang, klik dua kali untuk langsung menayangkan.
- Slide dibagi otomatis 1–3 ayat menurut panjangnya, dengan nomor ayat kecil di depan setiap ayat. Pilihan **1 ayat/slide** juga tersedia. Baris dipotong supaya teks sebesar mungkin di layar 16:9.
- Ayat yang dilihat atau ditayangkan tidak masuk pustaka. **+ Susunan** menyimpannya ke pustaka profil sebagai materi Ayat, misalnya "Yohanes 3:16-18 (TB)", lalu menambahkannya ke susunan ibadah. **Edit materi** juga menyimpannya ke pustaka dulu. Ayat yang sama tidak disimpan dua kali.
- **Impor Alkitab** lewat tombol di tab Alkitab atau menu **Impor → Alkitab lengkap**: OpenSong XML, Zefania XML, atau TSV UTF-8 (`kitab [TAB] pasal [TAB] ayat [TAB] teks`, atau `referensi [TAB] teks`). Ayat gabungan seperti `5-6` didukung. Kitab di luar 66 kitab, nomor ganda, dan ayat kosong dilewati dan jumlahnya disebutkan. Singkatan (misalnya TB) dan nama terjemahan diisi sebelum disimpan.
- Terjemahan disimpan di `alkitab.db` di folder data, berlaku untuk semua profil di komputer ini, dan tidak ikut sinkron. Tombol tempat sampah menghapusnya lagi.
- Lentera tetap tidak menyertakan teks Alkitab. Gunakan terjemahan yang Anda berhak pakai.

**Profil:** satu komputer kini bisa memakai beberapa profil, masing-masing dengan pustaka lirik, ayat, dan susunan ibadahnya sendiri. Data tersimpan per profil dalam basis data SQLite.

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

**Memperbarui:** pengguna 0.1.1, 0.2.0, dan 0.2.1 menerima versi ini lewat pembaruan otomatis; pengguna 0.1.0 memasang installer sekali. Pustaka lama pindah ke **Profil utama** saat 0.3.0 pertama kali dibuka. Program kini sekitar 12,8 MB (0.2.1: 11,2 MB).

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
