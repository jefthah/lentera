# Deploy server Lentera

Server Lentera berjalan di VPS sebagai dua container Docker: PostgreSQL dan API. Caddy di host menangani HTTPS dan meneruskan permintaan ke API di `127.0.0.1:8310`.

| Berkas | Isi |
|---|---|
| `../Dockerfile` | Image API: program Go statis di atas distroless, user nonroot, health check `lentera-server health` |
| `compose.yml` | PostgreSQL (tanpa port ke host, 512 MB), API (`127.0.0.1:8310`, 256 MB, hanya-baca), dan migrasi |
| `env.contoh` | Contoh `.env`: kata sandi database, rahasia kode masuk, kunci Resend, pengirim email |
| `deploy.sh` | Bangun image, cadangkan database, migrasi, nyalakan API, lalu kembali ke versi sebelumnya bila API baru tidak sehat |
| `cadangan.sh` | `pg_dump` ke `cadangan/`, simpan 14 hari; kirim juga ke luar VPS bila `cadangan-luar.env` ada |
| `cadangan-luar.sh` | Bungkus dump, enkripsi dengan age, unggah dengan rclone (misalnya ke Google Drive), simpan 60 hari |
| `cadangan-luar.contoh` | Contoh `cadangan-luar.env`: tujuan rclone, config rclone, public key age, masa simpan |
| `lentera.caddy` | Contoh situs Caddy |

CI (`.github/workflows/server.yml`, job **Image Docker dan alur deploy**) menjalankan `deploy.sh` dua kali di runner Linux, lalu memeriksa health, cadangan, dan pemulihannya. Workflow itu tidak men-deploy ke VPS.

Setelah server aktif dan alamatnya diisi di build rilis, **semua pengguna Lentera wajib masuk saat pertama kali membuka versi baru**. Pastikan server dan email kode sudah diuji sebelum merilis.

## Yang perlu disiapkan

1. **Domain API**, misalnya `api.domain-anda`. Buat record DNS `A` (dan `AAAA` bila VPS punya IPv6) yang mengarah ke VPS.
2. **Resend:** tambahkan domain pengirim di Resend, lalu pasang record DNS yang ditampilkan Resend (SPF, DKIM, dan MX untuk pantulan) sampai statusnya **Verified**. Buat API key dengan izin kirim saja.
3. **VPS:**
   - Docker Engine dengan Docker Compose v2.20 atau lebih baru (`docker compose version`);
   - Git;
   - Caddy yang memuat situs dari `/etc/caddy/sites/`.
   - Port 8310 di `127.0.0.1` belum dipakai (`ss -ltn | grep 8310`). Bila sudah dipakai, isi `LENTERA_PORT` di `.env` dengan port lain dan samakan di situs Caddy.

## Pertama kali

Jalankan sebagai user yang boleh memakai Docker:

```bash
sudo mkdir -p /opt/lentera && sudo chown "$USER": /opt/lentera
git clone https://github.com/jefthah/lentera.git /opt/lentera/repo
cd /opt/lentera/repo/server/deploy
cp env.contoh .env && chmod 600 .env
```

Isi `.env`:

- `POSTGRES_PASSWORD`: hasil `openssl rand -hex 24`.
- `LOGIN_CODE_SECRET`: hasil `openssl rand -hex 32`.
- `RESEND_API_KEY`: kunci dari Resend.
- `MAIL_FROM`: pengirim dari domain yang sudah diverifikasi, misalnya `"Lentera <masuk@domain-anda>"`.

Lalu jalankan:

```bash
./deploy.sh
curl -fsS http://127.0.0.1:8310/v1/health
```

Jawabannya harus `{"database":"ok","status":"ok"}`.

Setelah itu pasang situs Caddy:

```bash
sudo cp lentera.caddy /etc/caddy/sites/lentera.caddy
sudo nano /etc/caddy/sites/lentera.caddy   # ganti api.example.org dengan domain API
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

Periksa dari komputer lain:

```bash
curl -fsS https://api.domain-anda/v1/health
```

## Uji sebelum dirilis

1. Jalankan Lentera dengan server baru dan folder data uji:

   ```powershell
   lentera.exe --server https://api.domain-anda --data-dir "$env:TEMP\lentera-uji"
   ```

   Masuk dengan email sungguhan, pastikan kode tiba (periksa juga folder spam), lalu ubah sesuatu dan tekan **Sinkron sekarang**.
2. Ulangi di komputer kedua dan pastikan perubahannya sampai.
3. Hapus akun uji bila tidak dipakai lagi.

## Mengaktifkan di rilis

Isi variabel repositori GitHub **`LENTERA_SERVER`** dengan `https://api.domain-anda` (Settings → Secrets and variables → Actions → Variables). Build rilis berikutnya akan meminta masuk. Sejak 0.7.0, workflow rilis berhenti bila variabel itu kosong.

## Memperbarui server

```bash
cd /opt/lentera/repo
git pull --ff-only
server/deploy/deploy.sh
```

`deploy.sh` mencadangkan database sebelum migrasi. Bila API baru tidak sehat dalam 2 menit, versi sebelumnya dinyalakan lagi. Migrasi yang sudah berjalan tidak dibatalkan, jadi migrasi baru harus tetap bisa dipakai versi sebelumnya.

## Cadangan dan pemulihan

Cadangan harian lewat `crontab -e` user yang menjalankan Docker:

```cron
15 2 * * * /opt/lentera/repo/server/deploy/cadangan.sh >> /opt/lentera/repo/server/deploy/cadangan.log 2>&1
```

Cadangan disimpan di `server/deploy/cadangan/` di VPS yang sama. Bila VPS hilang, cadangan itu ikut hilang, jadi kirim juga ke luar VPS (bagian berikut).

Untuk memulihkan:

```bash
cd /opt/lentera/repo/server/deploy
docker compose stop api
docker compose exec -T postgres pg_restore -U lentera -d lentera --clean --if-exists < cadangan/lentera-<waktu>.dump
docker compose start api
```

### Salinan ke luar VPS

`cadangan-luar.sh` membungkus dump dalam arsip `lentera-<waktu>.tar.age`, mengenkripsinya dengan [age](https://age-encryption.org), lalu mengunggahnya dengan [rclone](https://rclone.org), misalnya ke Google Drive. VPS hanya memegang public key age. Arsip hanya bisa dibuka dengan private key, yang disimpan di laptop dan password manager, bukan di VPS.

`.env` sengaja tidak ikut. Dump saja cukup untuk memulihkan server dengan `.env` baru. Rahasia kode masuk yang baru tidak membuat perangkat keluar, karena token sesi tidak bergantung padanya. Yang perlu dibuat ulang hanya kunci Resend.

Pemasangan (sekali):

1. Pasang alatnya: `sudo apt install age rclone`.
2. Di laptop, buat pasangan kunci age (`age-keygen -o lentera-cadangan-age.txt`). Simpan private key itu di password manager, lalu salin baris `age1…` (public key) ke VPS, misalnya `/opt/lentera/age-penerima.txt`.
3. Buat remote rclone, misalnya Google Drive dengan izin `drive.file` (`rclone config`, atau `rclone authorize "drive"` di laptop bila VPS tanpa browser).
4. Isi pengaturannya:

   ```bash
   cd /opt/lentera/repo/server/deploy
   cp cadangan-luar.contoh cadangan-luar.env
   nano cadangan-luar.env
   ./cadangan.sh                   # cadangan baru + kirim ke luar VPS
   ./cadangan-luar.sh --daftar     # lihat arsip di tujuan
   ```

Setelah itu, setiap kali `cadangan.sh` berjalan (cron harian dan sebelum migrasi di `deploy.sh`), arsipnya juga dikirim. Arsip di tujuan yang lebih lama dari 60 hari dihapus (`LUAR_SIMPAN_HARI`). Bila pengiriman gagal, `cadangan.log` mencatat `PERINGATAN`, dan cadangan lokal tetap ada.

Membuka arsip di laptop (unduh dulu dari Drive, atau `rclone copy`):

```bash
age -d -i lentera-cadangan-age.txt lentera-<waktu>.tar.age | tar -xf -
# hasil: cadangan/lentera-<waktu>.dump
```

Untuk membangun ulang di VPS baru: ikuti **Pertama kali** (`.env` baru dengan rahasia baru dan kunci Resend), salin dump ke `server/deploy/cadangan/`, lalu pulihkan seperti di atas.

## Memantau

```bash
cd /opt/lentera/repo/server/deploy
docker compose ps
docker compose logs --tail 100 api
```

Log tiap container dibatasi 3 × 10 MB. Tanpa Resend, kode masuk ditulis ke log. Di production Resend wajib, jadi kode tidak muncul di log.

## Keamanan

- Rahasia hanya ada di `.env` (izin 600) di VPS dan tidak ikut ke salinan luar VPS. Berkas itu, `cadangan-luar.env`, `cadangan/`, dan log tidak di-commit (lihat `.gitignore`).
- PostgreSQL tidak membuka port ke host. API hanya mendengarkan `127.0.0.1`. Firewall VPS tidak perlu diubah selain port 80/443 yang sudah dipakai Caddy.
- Container API berjalan sebagai user nonroot, dengan sistem berkas hanya-baca dan tanpa capability Linux.

## Belum ada

- Deploy otomatis dari GitHub Actions. Sekarang deploy dijalankan manual di VPS.
