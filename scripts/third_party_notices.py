"""Menyusun THIRD-PARTY-NOTICES.txt dari dependensi lentera.exe (Windows).

Jalankan dari akar repositori setiap kali dependensi berubah:

    python scripts/third_party_notices.py

Skrip memanggil `cargo tree` untuk daftar crate pihak ketiga yang benar-benar
ikut dibangun untuk lentera.exe (dependensi normal dan build, dengan fitur yang
aktif), lalu `cargo metadata` untuk lisensi dan folder crate, dan menyalin teks
lisensi dari folder crate masing-masing. Untuk crate berlisensi ganda yang
menawarkan MIT, teks MIT yang dipakai. Crate Slint dipakai dengan lisensi
Slint Royalty-free 2.0, jadi hanya teks lisensi itu yang disalin.
"""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "x86_64-pc-windows-msvc"
APP = "lentera-app"
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|copyright)", re.IGNORECASE)

HEADER = """\
Lentera uses the following third-party components. Their license notices follow.

The user interface is built with Slint (https://slint.dev), used under the
Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0. The
attribution required by that license is shown in Lentera's About dialog.

The Windows installer and updater (Setup.exe, Update.exe) are built by
Velopack 1.2.161 (MIT License, https://github.com/velopack/velopack).

Lentera includes SQLite (https://sqlite.org), compiled from source by the
libsqlite3-sys crate. SQLite is in the public domain.
"""

# Crate yang paketnya tidak menyertakan berkas lisensi. Teksnya diambil dari
# repositori resmi masing-masing dan disimpan di scripts/licenses/.
EXTRA_LICENSES = {
    "accesskit": "accesskit-LICENSE-MIT",
    "accesskit_consumer": "accesskit-LICENSE-MIT",
    "accesskit_windows": "accesskit-LICENSE-MIT",
    "accesskit_winit": "accesskit-LICENSE-APACHE",
    "clipboard-win": "clipboard-win-LICENSE",
    "lyon_algorithms": "lyon-LICENSE-MIT",
    "lyon_extra": "lyon-LICENSE-MIT",
    "lyon_geom": "lyon-LICENSE-MIT",
    "lyon_path": "lyon-LICENSE-MIT",
    "rspolib": "rspolib-LICENSE",
    "taffy": "taffy-LICENSE",
    "velopack": "velopack-LICENSE",
}

# Lisensi yang dipilih bila crate menawarkan beberapa pilihan, urut prioritas.
PREFERRED = ["MIT", "LicenseRef-Slint-Royalty-free-2.0"]


def shipped_packages(meta):
    # `cargo metadata` menggabungkan fitur dari semua crate di workspace, jadi
    # daftarnya bisa memuat crate yang tidak dibangun. `cargo tree` memakai
    # fitur yang benar-benar aktif untuk aplikasi ini.
    tree = subprocess.run(
        ["cargo", "tree", "--locked", "-p", APP, "-e", "normal,build", "--target", TARGET,
         "--prefix", "none", "-f", "{p}"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    used = set()
    for line in tree.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1].startswith("v"):
            used.add((parts[0], parts[1][1:]))
    third_party = [
        p for p in meta["packages"] if p["source"] and (p["name"], p["version"]) in used
    ]
    return sorted(third_party, key=lambda p: (p["name"].lower(), p["version"]))


def reuse_files(package, folder):
    """Crate bergaya REUSE menyimpan teks lisensi di LICENSES/<SPDX-id>.*"""
    expression = package.get("license") or ""
    alternatives = re.split(r"\s+OR\s+", expression.replace("(", "").replace(")", ""))
    files = sorted(os.listdir(folder))
    if " AND " not in expression:
        for wanted in PREFERRED:
            if wanted in alternatives:
                chosen = [f for f in files if os.path.splitext(f)[0] == wanted]
                if chosen:
                    return chosen
    return files


def license_files(package):
    folder = os.path.dirname(package["manifest_path"])
    reuse = os.path.join(folder, "LICENSES")
    if os.path.isdir(reuse):
        return reuse, reuse_files(package, reuse)
    files = sorted(
        f
        for f in os.listdir(folder)
        if LICENSE_FILE.match(f) and os.path.isfile(os.path.join(folder, f))
    )
    if not files:
        extra = os.path.join(ROOT, "scripts", "licenses")
        name = EXTRA_LICENSES.get(package["name"])
        return extra, [name] if name else []
    expression = package.get("license") or ""
    alternatives = re.split(r"\s+OR\s+|/", expression.replace("(", "").replace(")", ""))
    if " AND " not in expression and "MIT" in alternatives:
        mit = [f for f in files if "mit" in f.lower()]
        if mit:
            return folder, mit
    return folder, files


def main():
    output = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout
    packages = shipped_packages(json.loads(output))

    parts = [HEADER, "\n==== Summary ====\n"]
    for p in packages:
        parts.append(f"{p['name']} {p['version']}: {p.get('license') or 'unknown'}\n")

    missing = []
    for p in packages:
        folder, files = license_files(p)
        if not files:
            missing.append(p["name"])
            parts.append(f"\n==== {p['name']}-{p['version']} ====\n")
            parts.append(
                f"License: {p.get('license') or 'unknown'}. The crate package does not "
                f"include a license file; see {p.get('repository') or 'crates.io'}.\n"
            )
            continue
        for name in files:
            with open(os.path.join(folder, name), encoding="utf-8", errors="replace") as f:
                text = f.read().replace("\r\n", "\n").strip("\n")
            parts.append(f"\n==== {p['name']}-{p['version']} / {name} ====\n{text}\n")

    with open(os.path.join(ROOT, "THIRD-PARTY-NOTICES.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("".join(parts))
    print(f"{len(packages)} crate; tanpa berkas lisensi: {', '.join(missing) or '-'}", file=sys.stderr)


if __name__ == "__main__":
    main()
