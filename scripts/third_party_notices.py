"""Menyusun THIRD-PARTY-NOTICES.txt dari dependensi lentera.exe (Windows).

Jalankan dari akar repositori setiap kali dependensi berubah:

    python scripts/third_party_notices.py

Skrip memanggil `cargo metadata`, mengambil semua crate pihak ketiga yang ikut
dibangun ke dalam lentera.exe (dependensi normal dan build), lalu menyalin teks
lisensi dari folder crate masing-masing. Untuk crate berlisensi ganda yang
menawarkan MIT, teks MIT yang dipakai.
"""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "x86_64-pc-windows-msvc"
APP = "lentera-win32"
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|copyright)", re.IGNORECASE)

HEADER = """\
Lentera uses the following third-party components. Their license notices follow.

The Windows installer and updater (Setup.exe, Update.exe) are built by
Velopack 1.2.161 (MIT License, https://github.com/velopack/velopack).
"""


def shipped_packages(meta):
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == APP)
    seen, stack = set(), [root]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            if any(kind["kind"] in (None, "build") for kind in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    third_party = [packages[i] for i in seen if packages[i]["source"]]
    return sorted(third_party, key=lambda p: (p["name"].lower(), p["version"]))


def license_files(package):
    folder = os.path.dirname(package["manifest_path"])
    files = sorted(
        f
        for f in os.listdir(folder)
        if LICENSE_FILE.match(f) and os.path.isfile(os.path.join(folder, f))
    )
    if not files:
        # Crate tanpa berkas lisensi di paketnya; teks diambil dari repositori
        # resminya dan disimpan di scripts/licenses/<nama>-LICENSE.
        extra = os.path.join(ROOT, "scripts", "licenses")
        files = sorted(f for f in os.listdir(extra) if f.startswith(package["name"] + "-"))
        return extra, files
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
