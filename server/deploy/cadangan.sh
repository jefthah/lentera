#!/bin/sh
# Mencadangkan database Lentera ke cadangan/lentera-<waktu UTC>.dump (format
# custom pg_dump) dan menghapus cadangan yang lebih lama dari SIMPAN_HARI hari
# (bawaan 14). deploy.sh menjalankannya sebelum migrasi. Untuk cadangan harian,
# tambahkan ke crontab user yang menjalankan Docker, misalnya:
#
#   15 2 * * * /opt/lentera/repo/server/deploy/cadangan.sh >> /opt/lentera/repo/server/deploy/cadangan.log 2>&1
#
# Bila cadangan-luar.env ada, setiap cadangan juga dikirim ke luar VPS dalam
# arsip terenkripsi (lihat cadangan-luar.sh). Kegagalan langkah itu hanya
# menjadi peringatan: cadangan lokal tetap ada dan skrip ini tetap berhasil.
set -eu
cd "$(dirname "$0")"
umask 077

keep_days=${SIMPAN_HARI:-14}
mkdir -p cadangan
file="cadangan/lentera-$(date -u +%Y%m%d-%H%M%S).dump"
trap 'rm -f "$file.tmp"' EXIT

docker compose exec -T postgres pg_dump -U lentera -d lentera --format=custom > "$file.tmp"
mv "$file.tmp" "$file"
find cadangan -name 'lentera-*.dump' -mtime +"$keep_days" -delete

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cadangan $file ($(wc -c < "$file") byte)"

if [ -f cadangan-luar.env ]; then
    ./cadangan-luar.sh "$file" ||
        echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) PERINGATAN: salinan ke luar VPS gagal; cadangan lokal tetap ada: $file" >&2
fi
