# Prototipe Lentera dengan Slint

Prototipe ini **bukan** aplikasi Lentera. Tujuannya membuktikan bahwa Slint
layak dipakai sebelum Lentera dibangun ulang untuk Windows dan macOS:

1. Jendela output bisa tampil fullscreen di monitor/proyektor yang dipilih,
   sementara jendela operator tetap di layar utama.
2. Video MP4 bisa diputar sebagai latar dengan teks lirik di atasnya.
3. Pemakaian RAM dan CPU tetap rendah untuk PC gereja (RAM 2–4 GB).
4. Kode yang sama bisa di-build untuk macOS (lewat GitHub Actions).

## Isi

| Berkas | Isi |
|---|---|
| `ui/app.slint` | Tampilan: jendela operator dan jendela output |
| `src/main.rs` | Logika aplikasi, pemilihan monitor, dan uji otomatis `--selftest` |
| `src/video_win.rs` | Pemutar video Windows memakai Media Foundation |
| `src/probe_win.rs` | Pemeriksaan Windows untuk uji: posisi jendela, tangkapan jendela output sendiri, RAM, CPU |

Video di macOS belum dibuat di prototipe ini (rencananya AVFoundation).

## Menjalankan

```bash
cargo run --release
```

Pilihan:

| Argumen | Arti |
|---|---|
| `--renderer femtovg` | Gambar memakai GPU lewat OpenGL (bawaan) |
| `--renderer software` | Gambar tanpa GPU, untuk PC dengan kartu grafis lemah |
| `--selftest` | Uji otomatis, lalu tutup sendiri |
| `--video PATH` | File MP4 untuk diuji |
| `--monitor N` | Monitor tujuan (mulai dari 1). Bawaan: monitor bukan-utama terakhir |
| `--seconds N` | Lama pengukuran video (bawaan 8 detik) |
| `--out DIR` | Folder hasil uji (bawaan `hasil`) |

Video uji bisa dibuat dengan ffmpeg:

```bash
ffmpeg -f lavfi -i testsrc2=size=1920x1080:rate=30 -t 10 -c:v libx264 -pix_fmt yuv420p media/uji-1080p.mp4
```

Uji otomatis menulis `hasil/laporan.json` dan tangkapan `1-teks.png` serta
`2-video-teks.png`. Tangkapan hanya mengambil isi jendela output prototipe
sendiri, bukan layar lain.

## Hasil

Lihat [HASIL.md](HASIL.md).
