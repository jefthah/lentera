#!/bin/sh
# Mengirim satu cadangan Lentera ke luar VPS. Dump dibungkus dalam arsip tar,
# dienkripsi dengan age (VPS hanya memegang public key), diunggah dengan rclone,
# lalu arsip di tujuan yang lebih lama dari LUAR_SIMPAN_HARI hari dihapus. Isi
# arsip hanya bisa dibuka dengan private key age milik pemilik server.
#
# .env sengaja tidak ikut: dump saja cukup untuk memulihkan server dengan .env
# baru (rahasia kode masuk baru tidak membuat perangkat keluar, karena token sesi
# tidak bergantung padanya), dan kunci Resend tidak perlu keluar dari VPS.
#
#   ./cadangan-luar.sh                     kirim dump terbaru di cadangan/
#   ./cadangan-luar.sh cadangan/<x>.dump   kirim dump tertentu
#   ./cadangan-luar.sh --daftar            lihat arsip di tujuan
#
# Pengaturan ada di cadangan-luar.env (salin dari cadangan-luar.contoh).
# cadangan.sh memanggil skrip ini setelah setiap cadangan bila berkas itu ada.
set -eu
cd "$(dirname "$0")"
umask 077

if [ ! -f cadangan-luar.env ]; then
    echo "cadangan-luar.env belum ada. Salin dari cadangan-luar.contoh lalu isi." >&2
    exit 1
fi
. ./cadangan-luar.env
: "${LUAR_TUJUAN:?isi LUAR_TUJUAN di cadangan-luar.env}"
: "${LUAR_AGE_PENERIMA:?isi LUAR_AGE_PENERIMA di cadangan-luar.env}"
keep_days=${LUAR_SIMPAN_HARI:-60}

for tool in age rclone tar; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "$tool belum terpasang." >&2
        exit 1
    fi
done
if [ ! -s "$LUAR_AGE_PENERIMA" ]; then
    echo "Public key age tidak ditemukan: $LUAR_AGE_PENERIMA" >&2
    exit 1
fi

luar_rclone() {
    if [ -n "${LUAR_RCLONE_CONFIG:-}" ]; then
        rclone --config "$LUAR_RCLONE_CONFIG" "$@"
    else
        rclone "$@"
    fi
}

if [ "${1:-}" = "--daftar" ]; then
    luar_rclone lsf --format tsp --separator '  ' --include 'lentera-*.tar.age' "$LUAR_TUJUAN" | sort
    exit 0
fi

dump=${1:-$(ls -1 cadangan/lentera-*.dump 2>/dev/null | sort | tail -n 1)}
dump=cadangan/$(basename "${dump:-tidak-ada}")
if [ ! -f "$dump" ]; then
    echo "Dump tidak ditemukan ($dump); tidak ada yang dikirim." >&2
    exit 1
fi

name=$(basename "$dump" .dump).tar.age
tmp=$(mktemp cadangan/.luar-XXXXXX)
trap 'rm -f "$tmp" "$tmp.gagal"' EXIT

# dash (sh di Ubuntu) tidak punya pipefail, jadi kegagalan tar ditandai lewat
# berkas. Arsip tidak pernah ditulis ke disk tanpa enkripsi.
{ tar -cf - "$dump" || : > "$tmp.gagal"; } | age -R "$LUAR_AGE_PENERIMA" -o "$tmp"
if [ -e "$tmp.gagal" ]; then
    echo "Arsip gagal dibuat; tidak ada yang dikirim." >&2
    exit 1
fi

luar_rclone copyto "$tmp" "$LUAR_TUJUAN/$name"
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) luar $LUAR_TUJUAN/$name ($(wc -c < "$tmp") byte)"

luar_rclone delete --min-age "${keep_days}d" --include 'lentera-*.tar.age' "$LUAR_TUJUAN" ||
    echo "PERINGATAN: arsip lama di $LUAR_TUJUAN gagal dihapus." >&2
