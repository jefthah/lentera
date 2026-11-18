"""Membuat crates/lentera-app/assets/icon/lentera.ico dari logo SVG.

Jalankan dari akar repositori setiap kali lentera.svg atau lentera-kecil.svg
berubah, lalu commit lentera.ico:

    python scripts/buat_ikon.py

Setiap ukuran digambar ulang dari SVG (bukan diperkecil dari 256 px) oleh
Chrome atau Edge mode headless, lalu disusun menjadi .ico dengan Pillow.
Ukuran 16–32 px memakai lentera-kecil.svg yang garisnya lebih tebal; ukuran
40 px ke atas memakai lentera.svg. Ikon dipasang ke lentera.exe oleh
crates/lentera-app/build.rs dan ke Setup.exe oleh workflow rilis
(`vpk pack --icon`).

Butuh Python 3.10+, Pillow, dan Google Chrome atau Microsoft Edge. Lokasi
browser lain bisa diberikan lewat variabel lingkungan CHROME.
"""

import argparse
import io
import os
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ICON_DIR = ROOT / "crates" / "lentera-app" / "assets" / "icon"
SMALL_SVG = ICON_DIR / "lentera-kecil.svg"
LARGE_SVG = ICON_DIR / "lentera.svg"
OUTPUT = ICON_DIR / "lentera.ico"
# Ukuran yang dipakai Windows pada skala layar 100–200 %.
SMALL_SIZES = [16, 20, 24, 32]
LARGE_SIZES = [40, 48, 64, 256]


def find_browser() -> str:
    candidates = [os.environ.get("CHROME")]
    for base in ("PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"):
        folder = os.environ.get(base)
        if folder:
            candidates.append(os.path.join(folder, "Google", "Chrome", "Application", "chrome.exe"))
            candidates.append(os.path.join(folder, "Microsoft", "Edge", "Application", "msedge.exe"))
    candidates += [shutil.which(name) for name in ("chrome", "google-chrome", "chromium", "msedge")]
    for path in candidates:
        if path and os.path.isfile(path):
            return path
    sys.exit("Chrome atau Edge tidak ditemukan. Isi variabel CHROME dengan lokasi chrome.exe.")


def render(browser: str, jobs: list[tuple[Path, int]], workdir: Path) -> list[Image.Image]:
    """Menggambar setiap (svg, ukuran) berdampingan dalam satu tangkapan layar."""
    gap = 8
    placed = []
    x = 0
    for svg, size in jobs:
        placed.append((x, svg, size))
        x += size + gap
    width, height = x, max(size for _, size in jobs)

    images = "".join(
        f'<img src="{svg.as_uri()}" width="{size}" height="{size}" style="left:{left}px">'
        for left, svg, size in placed
    )
    page = workdir / "ikon.html"
    page.write_text(
        "<!doctype html><html><head><style>"
        "html,body{margin:0;background:transparent}img{position:absolute;top:0}"
        f"</style></head><body>{images}</body></html>",
        encoding="utf-8",
    )
    shot = workdir / "ikon.png"
    subprocess.run(
        [
            browser,
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=1",
            "--default-background-color=00000000",
            f"--user-data-dir={workdir / 'profil'}",
            f"--window-size={width},{height}",
            f"--screenshot={shot}",
            page.as_uri(),
        ],
        check=True,
        capture_output=True,
    )
    sheet = Image.open(shot).convert("RGBA")
    if sheet.size != (width, height):
        sys.exit(f"Tangkapan layar berukuran {sheet.size}, seharusnya {(width, height)}.")
    if sheet.getpixel((width - 1, height - 1))[3] != 0:
        sys.exit("Latar tangkapan layar tidak transparan; browser ini tidak didukung.")
    return [sheet.crop((left, 0, left + size, size)) for left, _, size in placed]


def dib(image: Image.Image) -> bytes:
    """Bitmap 32-bit (BGRA, baris dari bawah) dengan mask AND, format ikon klasik."""
    width, height = image.size
    pixels = image.tobytes("raw", "BGRA")
    stride = width * 4
    rows = b"".join(pixels[y * stride : (y + 1) * stride] for y in reversed(range(height)))
    mask_stride = (width + 31) // 32 * 4
    alpha = image.getchannel("A")
    mask = bytearray()
    for y in reversed(range(height)):
        row = bytearray(mask_stride)
        for x in range(width):
            if alpha.getpixel((x, y)) == 0:
                row[x // 8] |= 0x80 >> (x % 8)
        mask += row
    header = struct.pack(
        "<IiiHHIIiiII", 40, width, height * 2, 1, 32, 0, len(rows) + len(mask), 0, 0, 0, 0
    )
    return header + rows + bytes(mask)


def png(image: Image.Image) -> bytes:
    buffer = io.BytesIO()
    image.save(buffer, "PNG", optimize=True)
    return buffer.getvalue()


def write_ico(path: Path, images: list[Image.Image]) -> None:
    # 256 px sebagai PNG (lebih kecil), ukuran lain sebagai bitmap supaya
    # terbaca oleh semua bagian Windows.
    frames = [(image.width, png(image) if image.width >= 256 else dib(image)) for image in images]
    data = bytearray(struct.pack("<HHH", 0, 1, len(frames)))
    offset = 6 + 16 * len(frames)
    for size, frame in frames:
        data += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(frame), offset)
        offset += len(frame)
    for _, frame in frames:
        data += frame
    path.write_bytes(data)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--png", type=Path, help="simpan juga tiap ukuran sebagai PNG di folder ini")
    args = parser.parse_args()

    jobs = [(SMALL_SVG, size) for size in SMALL_SIZES] + [(LARGE_SVG, size) for size in LARGE_SIZES]
    with tempfile.TemporaryDirectory(prefix="lentera-ikon-") as tmp:
        images = render(find_browser(), jobs, Path(tmp))
    write_ico(OUTPUT, images)
    print(f"{OUTPUT.relative_to(ROOT)}: {', '.join(str(i.width) for i in images)} px, "
          f"{OUTPUT.stat().st_size} byte")
    if args.png:
        args.png.mkdir(parents=True, exist_ok=True)
        for image in images:
            image.save(args.png / f"lentera-{image.width}.png")


if __name__ == "__main__":
    main()
