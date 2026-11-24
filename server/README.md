# Server Lentera

Server login dan sinkron untuk Lentera: login dengan kode email, profil, dan sinkron lirik, ayat, susunan ibadah, serta latar antarperangkat. Ditulis dengan Go dan PostgreSQL, mengikuti pola server JD Sign: `net/http`, pgx, migrasi goose yang tertanam di program, dan email lewat Resend.

Kontrak API untuk aplikasi ada di [docs/SINKRON.md](../docs/SINKRON.md). Aplikasi Lentera sudah bisa memakai server ini, tetapi server belum di-deploy, jadi build rilis belum mengaktifkan layar masuk.

## Menjalankan di komputer sendiri

Butuh Go 1.26 dan PostgreSQL 18. Dengan Docker:

```bash
docker compose -f compose.dev.yml up -d
go run ./cmd/lentera-server migrate up
go run ./cmd/lentera-server
```

API berjalan di `http://127.0.0.1:8310`. Tanpa `RESEND_API_KEY`, kode masuk tidak dikirim lewat email tetapi ditulis ke log server. Variabel environment lain ada di [.env.example](.env.example). Tanpa Docker, isi `DATABASE_URL` dengan alamat PostgreSQL yang sudah ada.

| Perintah | Fungsi |
|---|---|
| `lentera-server` / `lentera-server serve` | Menjalankan API |
| `lentera-server migrate up` | Menerapkan migrasi yang belum dijalankan |
| `lentera-server migrate down` | Membatalkan migrasi terakhir |
| `lentera-server migrate status` | Menampilkan status migrasi |
| `lentera-server health` | Memeriksa `/v1/health` API yang berjalan di `HTTP_ADDR`; keluar dengan kode 1 bila tidak sehat |

## Tes

```bash
TEST_DATABASE_URL="postgres://lentera:lentera-dev@127.0.0.1:54330/lentera?sslmode=disable" go test ./...
```

Setiap tes yang butuh database membuat database sementara sendiri lalu menghapusnya. Tanpa `TEST_DATABASE_URL`, tes itu dilewati. CI (`.github/workflows/server.yml`) menjalankan `gofmt`, `go vet`, `go test -race` dengan PostgreSQL 18, lalu build.

### Menguji aplikasi dengan server ini

Jalankan aplikasi dengan server lokal dan folder data uji, jangan dengan folder data ibadah:

```powershell
.\target\release\lentera.exe --server http://127.0.0.1:8310 --data-dir ".\uji-a"
```

Kode masuk muncul di log server. Token disimpan di Windows Credential Manager dengan nama `http://127.0.0.1:8310|<folder data>`. Hapus lewat **Keluar dari akun**, atau dengan `cmdkey /delete:<nama>` setelah mencarinya di `cmdkey /list`.

Uji integrasi klien Rust (`crates/lentera-core/tests/server.rs`) butuh token sesi akun uji:

1. `POST /v1/auth/code` dengan email uji.
2. Baca kodenya di log server.
3. `POST /v1/auth/verify` dengan email, kode, dan `device`. Simpan `token` dari jawabannya di berkas lokal yang tidak di-commit.

Lalu jalankan:

```bash
LENTERA_TEST_SERVER=http://127.0.0.1:8310 LENTERA_TEST_TOKEN="$(cat token.txt)" \
  cargo test -p lentera-core --features http --test server
```

Tanpa kedua variabel itu, uji ini dilewati.

## Isi folder

| Folder | Isi |
|---|---|
| `cmd/lentera-server` | Program utama: `serve` dan `migrate` |
| `internal/auth` | Kode email 6 digit (HMAC, 10 menit, 5 percobaan) dan sesi perangkat 180 hari |
| `internal/library` | Profil, peran, dan sinkron: revisi per profil, cursor, dan aturan "perubahan terbaru menang" |
| `internal/httpapi` | Endpoint `/v1/...`, token Bearer, batas ukuran, dan batas laju |
| `internal/database` | Koneksi dan migrasi; skema di `migrations/` |
| `internal/mail` | Email kode lewat Resend, atau ke log bila belum ada kunci |
| `internal/config` | Variabel environment |
| `internal/testdb` | Database sementara untuk tes |
| `deploy` | Compose production, skrip deploy dan cadangan, contoh situs Caddy |

## Di production

Wajib: `APP_ENV=production`, `DATABASE_URL`, `LOGIN_CODE_SECRET` (minimal 32 karakter), `RESEND_API_KEY`, dan `MAIL_FROM` dengan domain yang sudah diverifikasi di Resend. Server mendengarkan di alamat lokal (`HTTP_ADDR`) dan diharapkan berada di belakang Caddy yang menangani HTTPS. Alamat IP perangkat dibaca dari `X-Forwarded-For` hanya bila koneksi datang dari alamat lokal/privat. Batas laju disimpan di memori, jadi jalankan satu proses saja.

Image Docker, compose production, skrip deploy dan cadangan, contoh situs Caddy, dan langkah-langkahnya ada di [deploy/](deploy/README.md). `lentera-server health` memeriksa API yang sedang berjalan dan dipakai sebagai health check container.
