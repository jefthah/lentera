# Pengujian Lentera 0.1.0

Pengujian dilakukan pada 16 November 2026 di satu komputer pengembangan Windows x64. Hasil ini tidak mewakili semua komputer lama.

## Verifikasi kode

- `cargo test --locked`: 7 pengujian lulus.
- `cargo clippy --all-targets --locked -- -D warnings`: tanpa peringatan.
- Build release berhasil, ukuran executable 368.128 byte.
- Dependensi executable hanya DLL sistem Windows; C runtime ditautkan statis.

Pengujian model memeriksa pemisahan slide dan Unicode, impor yang gagal tanpa perubahan parsial, duplikat ayat, referensi susunan setelah penghapusan, pencarian, penulisan / pembacaan data dan cadangan, serta penolakan data rusak / versi tidak didukung.

## Uji alur aplikasi

Sembilan pemeriksaan otomatis dijalankan pada dua dataset, semuanya lulus:

1. Menayangkan slide pertama.
2. Memindahkan ke slide berikutnya.
3. Memindahkan ke materi berikutnya dalam susunan.
4. Kembali ke materi sebelumnya.
5. Mengaktifkan blackout.
6. Beralih ke layar penuh.
7. Kembali ke mode jendela.
8. Mencari berdasarkan isi teks.
9. Mengedit judul Unicode dan memverifikasi hasil penyimpanan.

Windows melaporkan dua monitor saat pengujian. Keberhasilan pemeriksaan jendela otomatis belum membuktikan bahwa semua konfigurasi proyektor bekerja dengan benar.

## Pemakaian memori awal

Proses disampel setiap sekitar 100 ms sepanjang pengujian singkat sekitar 3,6 detik. Angka berikut adalah nilai puncak yang teramati selama sampling, bukan pengukuran jangka panjang.

| Dataset | Working set | Private memory |
|---|---:|---:|
| Dua materi contoh | 12,78 MiB | 2,03 MiB |
| Dua materi + 31.000 ayat sintetis | 27,00 MiB | 26,77 MiB |

Dataset besar memakai teks sintetis, bukan terjemahan Alkitab berlisensi.

## Yang masih perlu diuji

- Pemeriksaan visual manual menyeluruh.
- Komputer sasaran dengan RAM 2–4 GB dan perangkat keras lama.
- Pemakaian ibadah dalam durasi panjang.
- Resolusi, skala tampilan, dan proyektor tujuan.

Untuk mengulang uji alur setelah build, gunakan folder data percobaan baru yang terpisah:

```powershell
.\target\release\lentera.exe --data-dir ".\test-data" --ui-smoke-test
```

Hasil ditulis ke `test-data/ui-smoke-result.json`. Jangan gunakan folder data ibadah nyata untuk pengujian ini.
