#!/bin/sh
# Mencadangkan database Lentera ke cadangan/lentera-<waktu UTC>.dump (format
# custom pg_dump) dan menghapus cadangan yang lebih lama dari SIMPAN_HARI hari
# (bawaan 14). deploy.sh menjalankannya sebelum migrasi. Untuk cadangan harian,
# tambahkan ke crontab user yang menjalankan Docker, misalnya:
#
#   15 2 * * * /opt/lentera/repo/server/deploy/cadangan.sh >> /opt/lentera/repo/server/deploy/cadangan.log 2>&1
#
# Cadangan ini masih di VPS yang sama. Salin juga ke tempat lain secara berkala.
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
