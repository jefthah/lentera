#!/bin/sh
# Memasang atau memperbarui server Lentera di VPS dari commit yang sedang
# di-checkout. Jalankan dari mana saja oleh user yang boleh memakai Docker:
#
#   git pull --ff-only && server/deploy/deploy.sh
#
# Urutan: bangun image, cadangkan database (bila sudah ada), jalankan migrasi,
# lalu nyalakan API baru dan tunggu sampai sehat. Bila API baru tidak sehat,
# API sebelumnya dinyalakan lagi. Migrasi yang sudah jalan tidak dibatalkan.
set -eu
cd "$(dirname "$0")"

if [ ! -f .env ]; then
    echo "Berkas .env belum ada. Salin dari env.contoh lalu isi rahasianya." >&2
    exit 1
fi

tag=${LENTERA_TAG:-$(git rev-parse --short=12 HEAD)}
before=$(cat .tag-aktif 2>/dev/null || true)
export LENTERA_TAG="$tag"

echo "== Membangun image lentera-server:$tag"
docker compose build api

if [ -n "$(docker compose ps --status running --quiet postgres)" ]; then
    echo "== Mencadangkan database sebelum migrasi"
    ./cadangan.sh
fi

echo "== Menyalakan PostgreSQL"
docker compose up -d --wait postgres

echo "== Menjalankan migrasi"
docker compose run --rm migrate

echo "== Menyalakan API"
if docker compose up -d --wait --wait-timeout 120 api; then
    echo "$tag" > .tag-aktif
    echo "== Server Lentera berjalan dengan lentera-server:$tag"
else
    echo "API lentera-server:$tag tidak sehat." >&2
    docker compose logs --no-color --tail 50 api >&2 || true
    if [ -n "$before" ] && [ "$before" != "$tag" ]; then
        echo "Kembali ke lentera-server:$before. Migrasi tidak dibatalkan." >&2
        LENTERA_TAG="$before" docker compose up -d --wait --wait-timeout 120 api || true
    fi
    exit 1
fi

# Simpan image yang aktif dan satu sebelumnya untuk kembali; hapus sisanya.
docker image ls lentera-server --format '{{.Tag}}' |
    grep -vx -e "$tag" -e "${before:-$tag}" |
    while read -r old; do
        docker image rm "lentera-server:$old" >/dev/null || true
    done
