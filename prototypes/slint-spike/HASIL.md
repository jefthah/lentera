# Hasil prototipe Slint — 17 November 2026

## Kesimpulan singkat

Slint **layak** dipakai untuk Lentera di Windows. Ketiga risiko utama terbukti
bisa diatasi di Windows. Dua hal belum terbukti: build dan jendela di macOS
(menunggu GitHub Actions), serta kinerja di PC gereja yang lemah.

| Risiko | Hasil di Windows | macOS |
|---|---|---|
| Output fullscreen di monitor pilihan | Lulus di semua uji | Belum diuji (CI) |
| Video + teks lirik di atasnya | Lulus, 30 fps, tanpa frame terbuang | Pemutar AVFoundation belum dibuat |
| RAM dan CPU rendah | 32 MiB tanpa video; 164 MiB dengan video 1080p (renderer software + decode GPU) | Belum diukur |

## Komputer uji

Ini komputer pengembangan yang **jauh lebih kencang** dari PC gereja target,
jadi angka CPU di bawah adalah batas bawah, bukan perkiraan untuk PC lemah.

- Intel Core i5-13500 (14 inti, 20 thread), RAM 32 GB, NVIDIA GeForce RTX 4060 Ti
- Windows 11, dua monitor: 1920×1080 (utama) dan 1080×1920 potret di koordinat (1920, −592)
- Slint 1.19.0, Rust 1.100.0, `windows` 0.62.2
- Video uji: pola `testsrc2` dari ffmpeg, H.264 High, 30 fps, 10 detik, diulang terus

## Angka

Diukur oleh `--selftest`. RAM = working set proses; "private" = memori yang
dipesan proses. CPU = persen dari satu inti selama 8 detik video.

| Konfigurasi | RAM tanpa video | RAM video | CPU video | fps |
|---|---|---|---|---|
| Renderer software, decode GPU, 1080p | 31,9 MiB (private 14,1) | 164 MiB | 32% | 30 |
| Renderer femtovg (OpenGL), decode GPU, 1080p | 88,2 MiB (private 98,3) | 238 MiB | 27% | 30 |
| Renderer software, decode CPU, 1080p | 31,9 MiB | 240 MiB | 69% | 30 |
| Renderer femtovg, decode CPU, 1080p | 88,4 MiB | 321 MiB | 75% | 30 |
| Renderer software, decode GPU, 720p | 31,8 MiB | 123 MiB | 24% | 30 |
| Renderer femtovg, decode GPU, 720p | 88,4 MiB | 188 MiB | 19% | 30 |

Sebagai pembanding, Lentera v0.1.0 (Win32/GDI) memakai 12,8 MiB pada data kecil.

Ukuran program: **9,6 MB** (profil rilis dioptimalkan untuk ukuran), dibanding
0,36 MB untuk v0.1.0.

## Pemeriksaan otomatis

Semua konfigurasi lulus:

- **Posisi:** menurut winit dan menurut Win32 (`MonitorFromWindow`/`GetWindowRect`),
  jendela output tepat menutupi monitor kedua, termasuk monitor potret dengan
  koordinat negatif.
- **Teks terlihat:** ±7% piksel terang pada tangkapan jendela output.
- **Video bergerak:** dua tangkapan berjarak 0,4 detik berbeda.
- **Decode GPU:** frame benar-benar berasal dari permukaan GPU (DXVA lewat Direct3D 11).
- **Crop:** video 1080p tampil 1920×1080. Sebelum diperbaiki, decoder memberi
  1088 baris dan ada strip hijau di bawah.

Uji layar hitam gagal di renderer femtovg setelah video diputar. Penyebabnya
**bukan Slint**: tangkapan menunjukkan notifikasi "Press Alt+Z to open NVIDIA
Overlay" yang disisipkan driver NVIDIA ke aplikasi OpenGL fullscreen. Dengan
renderer software, notifikasi itu tidak muncul.

## Temuan penting

1. **Renderer software sebaiknya jadi bawaan untuk PC lemah.** RAM-nya paling
   kecil, tidak bergantung pada driver OpenGL, dan tidak memicu overlay NVIDIA di
   proyektor. Renderer femtovg menambah ±56 MiB hanya karena driver OpenGL.
2. **Decode video harus lewat GPU.** CPU turun dari ±70% ke ±30% satu inti untuk
   1080p, dan RAM turun ±75 MiB. Hampir semua GPU Intel sejak 2011 mendukung
   decode H.264.
3. **Notifikasi overlay GPU bisa muncul di proyektor** dengan renderer OpenGL.
   Panduan Lentera perlu menjelaskan cara mematikannya, atau output memakai
   renderer software.
4. **Program jadi ±27× lebih besar** (9,6 MB). Masih kecil untuk unduhan, dan
   bisa dikurangi nanti dengan menganalisis isi binary.

## Yang belum terbukti

- **macOS:** build, posisi jendela, dan pemutar video (AVFoundation belum ditulis).
- **PC gereja lemah** (2–4 GB RAM, Intel HD, prosesor 2–4 inti): renderer software
  menggambar ulang seluruh layar setiap frame video, jadi CPU bisa jauh lebih
  tinggi. Pada PC lemah mungkin renderer GPU lebih cocok; ini harus diuji di sana.
- **fps yang benar-benar tergambar** di renderer software: Slint tidak menyediakan
  notifikasi render untuk renderer ini. Gerakan video dibuktikan lewat perbedaan
  tangkapan, bukan hitungan frame.
- Video 4K, audio, PDF, dan pengujian visual oleh manusia di proyektor sungguhan.

## Langkah berikutnya

1. Jalankan workflow `.github/workflows/prototipe-slint.yml` untuk build dan uji
   di macOS dan Windows di GitHub Actions.
2. Coba `--selftest` di PC gereja target dengan kedua renderer.
3. Kalau keduanya baik: tetapkan Slint, lalu mulai memecah repo menjadi core + app.
