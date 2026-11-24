# Login dan sinkron Lentera — protokol API

Dokumen ini untuk pengembang. Isinya kontrak antara aplikasi Lentera dan server di folder [`server/`](../server/README.md), lalu cara aplikasi memakainya (bagian [Di aplikasi Lentera](#di-aplikasi-lentera)). Server sudah di-deploy dengan berkas di [`server/deploy/`](../server/deploy/README.md). Build rilis mengaktifkan layar masuk setelah variabel repositori `LENTERA_SERVER` diisi.

## Prinsip

- **Offline lebih dulu.** Aplikasi tetap menyimpan semuanya di SQLite per profil dan bisa dipakai tanpa internet. Server hanya tempat bertukar perubahan antarperangkat. Sesi yang berakhir atau internet yang mati tidak boleh mengunci aplikasi saat ibadah.
- **Id dibuat aplikasi.** Id profil (sama dengan nama foldernya) dan id materi (`uid` di `library.db`) berupa UUID v7 yang dibuat aplikasi, jadi tidak perlu bertanya ke server dulu.
- **Waktu dalam milidetik.** Semua `created_at`, `updated_at`, dan `deleted_at` adalah milidetik sejak 1970 (UTC), sama seperti di SQLite.
- **Perubahan terbaru menang.** Bentrok diputuskan per materi oleh `updated_at` yang lebih besar. Bila sama, versi server dipertahankan. Perangkat yang kalah menerima versi pemenang, jadi semua perangkat berakhir dengan data yang sama.
- **Hapus = tanda hapus.** Materi yang dihapus dikirim dengan `deleted_at` terisi. Server mengosongkan judul dan isinya, lalu meneruskan tanda hapus itu ke perangkat lain.

## Login

Login memakai kode 6 digit dari email. Tidak ada kata sandi.

1. `POST /v1/auth/code` dengan `{"email": "nama@gereja.org"}` mengirim kode, lalu menjawab `{"email", "sent", "seconds_until_resend", "expires_in_seconds"}`. Kode berlaku 10 menit. Dalam 60 detik setelah kode dikirim, permintaan berikutnya tidak mengirim email baru (`sent: false`).
2. `POST /v1/auth/verify` dengan `{"email", "code", "device"}`. `device` adalah nama perangkat untuk daftar sesi nanti, misalnya nama komputer. Jawabannya `{"token": "...", "account": {"id", "email"}}`. Email yang belum terdaftar langsung mendapat akun baru. Setelah 5 kali salah, kode dibuang.
3. Kirim token di setiap permintaan lain: `Authorization: Bearer <token>`. Simpan token di penyimpanan rahasia sistem operasi (Windows Credential Manager / macOS Keychain), bukan di `library.db`.

Sesi berlaku 180 hari sejak terakhir dipakai dan diperpanjang otomatis. `POST /v1/auth/logout` mengakhiri sesi perangkat ini. `GET /v1/me` mengembalikan akun yang sedang masuk. Jawaban `401` berarti aplikasi perlu meminta login lagi; data lokal tetap dipakai sampai itu.

## Profil

- `GET /v1/profiles` → `{"profiles": [{"id", "name", "role", "created_at", "name_updated_at", "revision"}]}`. Isinya hanya profil yang belum dihapus dan bisa diakses akun ini.
- `PUT /v1/profiles/{id}` dengan `{"name", "created_at", "updated_at"}` membuat profil dengan id itu (`201`, peran `owner`) atau mengganti namanya bila `updated_at` lebih baru daripada `name_updated_at` di server (`200`). Jawabannya selalu profil seperti tersimpan di server. Pakai nama itu bila berbeda dari nama lokal.
- `DELETE /v1/profiles/{id}` menghapus profil untuk semua anggotanya (`204`). Hanya `owner` yang boleh.

Aturan nama sama dengan aplikasi: spasi dirapikan, wajib diisi, dan paling panjang 40 karakter. Server tidak mewajibkan nama unik, karena satu profil bisa dipakai beberapa akun.

**Peran:** `owner` (semua), `editor` (ubah isi dan nama), `viewer` (hanya menerima). Satu akun bisa punya banyak profil dan satu profil bisa dipakai beberapa akun, jadi bentuk ini cocok untuk profil per orang, per pelayanan, atau per gereja. Endpoint untuk mengundang anggota belum ada.

## Sinkron

`POST /v1/profiles/{id}/sync`

```json
{
  "cursor": 0,
  "items": [
    {"id": "0190d2a0-…", "kind": "song", "title": "Kasih-Mu", "content": "Bait satu\n\nBait dua",
     "created_at": 1763550000000, "updated_at": 1763550000000, "deleted_at": null}
  ],
  "service": {"items": ["0190d2a0-…", "0190d2a0-…"], "updated_at": 1763550000000},
  "settings": {"background": 2, "updated_at": 1763550000000}
}
```

- `cursor`: revisi server terakhir yang sudah diterima perangkat ini untuk profil ini (0 = belum pernah).
- `items`: materi yang berubah di perangkat ini sejak sinkron terakhir (`kind`: `song` atau `verse`).
- `service`: urutan susunan ibadah lengkap. Kirim hanya bila berubah, dan isi `null` bila tidak. Materi yang sama boleh muncul lebih dari sekali.
- `settings`: latar layar. Kirim hanya bila berubah. **Pilihan monitor tidak dikirim**, karena itu milik perangkat, bukan profil.

Jawaban:

```json
{"cursor": 7, "more": false, "items": [...], "service": null, "settings": null,
 "accepted": 3, "server_time": 1763550001234}
```

- `items` berisi perubahan dari perangkat lain sejak `cursor`, ditambah versi server untuk materi kiriman yang kalah bentrok atau waktunya dikoreksi. Terapkan semuanya ke SQLite lokal apa adanya, termasuk `updated_at`.
- `service` / `settings` terisi bila ada versi lebih baru dari perangkat lain, atau kiriman perangkat ini kalah. `null` berarti tidak ada perubahan.
- Simpan `cursor` baru setelah semua isi jawaban tersimpan di SQLite (dalam satu transaksi), bukan sebelumnya.
- `more: true` berarti masih ada halaman berikutnya. Ulangi dengan `cursor` baru. `service` bisa menyebut materi di halaman berikutnya, jadi terapkan `service` terakhir setelah `more` menjadi `false`.
- `server_time` bisa dipakai untuk memperingatkan bila jam komputer jauh berbeda.
- Kiriman perangkat sendiri tidak dikirim balik. Pengecualiannya saat jawaban dipecah per halaman: kiriman sendiri bisa muncul lagi di halaman berikutnya. Menerapkannya ulang tidak mengubah apa-apa karena isinya sama.

### Urutan sinkron di aplikasi

1. Tarik dulu: kirim `items: []` dan ulangi sampai `more: false`.
2. Kirim perubahan lokal dalam beberapa bagian, paling banyak 1.000 materi dan sekitar 8 MB per permintaan. Kirim `service` di bagian terakhir, setelah semua materi yang disebutnya terkirim.
3. Tandai materi lokal sebagai sudah terkirim hanya setelah server menjawab `200`.

### Aturan bentrok

- Materi: `updated_at` yang lebih besar menang. Bila sama, versi server dipertahankan. `created_at` pertama yang diterima server tetap dipakai.
- Susunan ibadah dan latar: diputuskan dengan cara yang sama, masing-masing sebagai satu kesatuan.
- Waktu lebih dari 10 menit di depan jam server diganti dengan waktu server, supaya satu komputer dengan jam salah tidak selalu menang. Versi yang sudah dikoreksi dikirim balik di jawaban.

### Batas

| Hal | Batas |
|---|---|
| Nama profil | 40 karakter |
| Judul materi | 1.000 karakter |
| Isi materi | 1 MB (UTF-8) |
| Materi per permintaan sinkron | 1.000 |
| Ukuran permintaan sinkron | 16 MB (permintaan lain 1 MB) |
| Susunan ibadah | 1.000 baris |
| Latar | 0–15 |
| Profil milik satu akun | 100 |
| Satu halaman jawaban | 1.000 materi atau sekitar 4 MB |

Materi yang melewati batas ditolak bersama seluruh permintaan (`422`), jadi aplikasi perlu memeriksa batas ini sebelum mengirim dan memberi tahu pengguna materi mana yang terlalu besar.

Judul dan isi juga harus UTF-8 yang sah dan tidak boleh berisi karakter NUL. Pesan galatnya menyebut "karakter yang tidak sah", bukan ukuran. Aplikasi membuang karakter kontrol selain baris baru dan tab sebelum menyimpan dan sebelum mengirim. Karakter seperti itu, misalnya NUL di lirik EasyWorship 7, tidak tampil di layar.

## Di aplikasi Lentera

Logika sinkron ada di `crates/lentera-core/src/sync/`: `Engine` menjalankan satu putaran sebagai urutan permintaan, dan klien HTTP (`HttpApi`) hanya ikut dengan fitur `http`. Tampilan dan alurnya ada di `crates/lentera-app/src/akun.rs`.

### Alamat server

- Dibaca saat build dari variabel environment `LENTERA_SERVER`. Workflow rilis mengambilnya dari variabel repositori dengan nama yang sama, dan sejak 0.7.0 berhenti bila variabel itu kosong.
- Bila kosong, layar masuk dan sinkron tidak aktif dan Lentera berjalan seperti 0.6.0.
- Untuk uji, `lentera.exe --server http://127.0.0.1:8310` memakai server lain, dan `--server ""` mematikannya.
- Uji alur UI (`--ui-smoke-test`) hanya memakai server bila `--server` diberikan.

### Masuk

- Selama folder data belum punya `akun.json`, jendela **Masuk ke Lentera** muncul sebelum layar operator: email, lalu kode 6 digit.
- Bila server tidak terjangkau (offline atau `5xx`) dan folder data sudah berisi profil, ada tombol **Buka tanpa masuk untuk sementara**. Lentera lalu terbuka dengan status **Belum masuk**. Tanpa data lokal, tombol ini tidak ada.
- **Folder data kosong:** setelah masuk, profil akun diunduh dulu (langkah "Menyiapkan profil") sebelum folder data dibuka, supaya Profil utama contoh tidak ikut terkirim. Akun yang belum punya profil mendapat Profil utama baru, yang lalu dikirim ke akun.
- **Folder data yang sudah berisi** langsung dibuka, dan semua profilnya ditambahkan ke akun pada sinkron pertama. Profil dari server yang namanya sudah dipakai profil lokal diberi akhiran " (2)", " (3)", dan seterusnya.
- **Masuk lagi** (setelah `401`) harus memakai akun yang sama. Akun lain ditolak, dan sesi barunya langsung diakhiri.
- **Keluar dari akun** menghapus token dan `akun.json`, mengakhiri sesi di server, lalu menutup Lentera. Data profil tetap ada.

### Penyimpanan

- **Token sesi:** di Windows Credential Manager / macOS Keychain, dengan layanan `Lentera` dan nama `{server}|{folder data}`. Karena itu folder data uji (`--data-dir`) tidak memakai sesi folder utama.
- **`akun.json` di folder data:** alamat server, id dan email akun, serta daftar profil yang dihapus di perangkat ini. Token tidak disimpan di sini.
- **`library.db`:** catatan sinkron per profil ada di tabel `synced` dan kunci `meta` `sync_*`. Semuanya tambahan pada skema 2, jadi profil tetap bisa dibuka Lentera 0.5 dan 0.6.

### Kapan sinkron berjalan

- Saat aplikasi dibuka, lalu setiap 30 detik.
- Setelah profil diganti namanya, dibuka, atau dihapus.
- Saat pengguna menekan **Sinkron sekarang**.

Permintaan HTTP berjalan di thread lain. Basis data hanya diubah di thread tampilan, dan perubahan disimpan dulu sebelum setiap permintaan. Bila sinkron mengubah profil yang sedang dibuka, pustaka, susunan, dan latarnya dibaca ulang. Slide yang sedang tayang tidak berubah.

### Status

Tombol akun di bilah atas menampilkan salah satu status ini:

- **Menyinkron…** (hanya bila putaran berjalan lebih dari 0,8 detik)
- **Tersinkron**
- **Offline**
- **Menunggu** (`429`)
- **Masuk lagi** (`401`)
- **Perlu perhatian**
- **Sinkron gagal**
- **Belum masuk**

Jam komputer yang berbeda lebih dari 5 menit dari `server_time` diperingatkan di menu akun.

### Profil yang dihapus

- **Dihapus di perangkat ini:** dicatat di `akun.json`. Profil itu tidak diunduh lagi, dan dihapus di server bila akun ini pemiliknya.
- **Dihapus pemiliknya di server (`410`) atau tidak bisa diakses lagi (`404`):** profil tetap ada di komputer ini tetapi tidak disinkron lagi. Statusnya **Perlu perhatian** sampai profil itu dihapus lewat **Pilih profil**.

### Belum ikut sinkron

- Pilihan monitor, karena itu milik perangkat.
- Gambar latar dan latar per materi. Latar per materi di perangkat ini tidak terhapus saat materinya diperbarui dari server.
- Alkitab yang diimpor (`alkitab.db`), termasuk **Terakhir dibuka**.

## Kesalahan

Setiap kesalahan berbentuk `{"error": {"code": "...", "message": "...", "field": "..."}}`. `message` berbahasa Indonesia dan boleh ditampilkan apa adanya.

| Status | `code` | Tindakan aplikasi |
|---|---|---|
| 400 | `bad_request` | Bug di aplikasi; catat. |
| 401 | `unauthorized` | Minta login lagi; tetap pakai data lokal. |
| 403 | `read_only`, `owner_only`, `account_disabled` | Tampilkan pesan. |
| 404 | `profile_not_found`, `not_found` | Profil tidak ada atau akun ini bukan anggotanya. |
| 409 | `cursor_ahead` | Server lebih lama daripada perangkat, misalnya setelah server dipulihkan dari cadangan. Ubah cursor ke 0 dan kirim ulang semua materi lokal. |
| 409 | `profile_taken`, `profile_busy` | Id profil bentrok; buat id baru atau coba lagi. |
| 410 | `profile_deleted` | Profil dihapus pemiliknya. Simpan sebagai profil lokal yang tidak disinkron lagi, lalu beri tahu pengguna. |
| 413 | `request_too_large` | Pecah kiriman jadi lebih kecil. |
| 422 | `validation_failed`, `service_unknown_item`, `code_expired`, `invalid_code`, `too_many_attempts`, `profile_limit` | Tampilkan pesan; `field` menunjuk bagian yang salah. |
| 429 | `rate_limited` | Tunggu sesuai header `Retry-After`. |
| 503 | `mail_failed` | Email kode gagal; coba lagi nanti. |

## Belum ada

- Mengundang anggota dan mengatur peran.
- Daftar sesi per perangkat dan keluar dari semua perangkat.
- Login Google.
- Latar video dan gambar di bucket.
- Sinkron Alkitab yang diimpor.
- Deploy otomatis dari GitHub Actions. Deploy masih dijalankan manual dengan [`server/deploy/deploy.sh`](../server/deploy/README.md).
- Cadangan database ke luar server.
