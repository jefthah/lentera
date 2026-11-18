fn main() {
    // Gaya gelap untuk kolom teks, daftar, dan pilihan bawaan Slint; sisanya
    // digambar sendiri di ui/app.slint.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("UI Slint gagal dikompilasi");

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());

        // Manifest Windows: DPI per monitor dan tampilan dialog modern.
        let manifest = dir.join("assets/app.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
        println!("cargo:rerun-if-changed=assets/app.manifest");

        // Ikon dan info versi lentera.exe. Ikon dipakai Explorer, shortcut,
        // title bar, dan taskbar (lihat `window_icons` di src/main.rs).
        let ico = std::fs::read(dir.join("assets/icon/lentera.ico"))
            .expect("assets/icon/lentera.ico tidak ada; jalankan python scripts/buat_ikon.py");
        let version = std::env::var("CARGO_PKG_VERSION").unwrap();
        let res =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("lentera.res");
        std::fs::write(&res, resource::build(&ico, &version)).expect("lentera.res gagal ditulis");
        // link.exe menerima berkas .res langsung sebagai masukan.
        println!("cargo:rustc-link-arg-bins={}", res.display());
        println!("cargo:rerun-if-changed=assets/icon/lentera.ico");
    }
}

/// Menulis berkas resource Windows (.res) tanpa rc.exe: grup ikon dari .ico
/// dan VERSIONINFO. Format mengikuti "Resource File Formats" dan
/// "VS_VERSIONINFO" di dokumentasi Win32.
mod resource {
    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    const RT_VERSION: u16 = 16;
    const LANG_EN_US: u16 = 0x0409;
    /// Nomor grup ikon; harus sama dengan `APP_ICON` di src/main.rs.
    const APP_ICON: u16 = 1;

    pub fn build(ico: &[u8], version: &str) -> Vec<u8> {
        assert!(
            ico.len() > 6 && word(ico, 0) == 0 && word(ico, 2) == 1,
            "lentera.ico bukan berkas ikon"
        );
        let mut out = Vec::new();
        // Setiap berkas .res diawali entri kosong.
        entry(&mut out, 0, 0, 0, 0, &[]);

        // Tiap gambar di .ico menjadi RT_ICON; RT_GROUP_ICON mendaftar semuanya.
        let count = word(ico, 4);
        let mut group = Vec::new();
        for value in [0, 1, count] {
            group.extend(value.to_le_bytes());
        }
        for index in 0..count {
            let at = 6 + usize::from(index) * 16;
            let size = dword(ico, at + 8) as usize;
            let offset = dword(ico, at + 12) as usize;
            let id = index + 1;
            entry(
                &mut out,
                RT_ICON,
                id,
                0x1010,
                LANG_EN_US,
                &ico[offset..offset + size],
            );
            group.extend_from_slice(&ico[at..at + 12]);
            group.extend(id.to_le_bytes());
        }
        entry(
            &mut out,
            RT_GROUP_ICON,
            APP_ICON,
            0x1030,
            LANG_EN_US,
            &group,
        );
        entry(
            &mut out,
            RT_VERSION,
            1,
            0x0030,
            LANG_EN_US,
            &version_info(version),
        );
        out
    }

    fn entry(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, lang: u16, data: &[u8]) {
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(32u32.to_le_bytes());
        for value in [0xFFFF, kind, 0xFFFF, name] {
            out.extend(value.to_le_bytes());
        }
        out.extend(0u32.to_le_bytes());
        out.extend(flags.to_le_bytes());
        out.extend(lang.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend_from_slice(data);
        pad(out);
    }

    fn version_info(version: &str) -> Vec<u8> {
        let mut parts = version
            .split(['.', '-', '+'])
            .map(|part| part.parse::<u32>().unwrap_or(0));
        let mut next = || parts.next().unwrap_or(0).min(0xFFFF);
        let (major, minor, patch) = (next(), next(), next());
        let (ms, ls) = ((major << 16) | minor, patch << 16);
        // VS_FIXEDFILEINFO: tanda tangan, versi struktur, versi berkas dan
        // produk, masker flag, OS = VOS_NT_WINDOWS32, tipe = VFT_APP.
        let fixed: Vec<u8> = [
            0xFEEF_04BD,
            0x0001_0000,
            ms,
            ls,
            ms,
            ls,
            0x3F,
            0,
            0x0004_0004,
            1,
            0,
            0,
            0,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();

        let strings: Vec<Vec<u8>> = [
            ("CompanyName", "Lentera contributors"),
            ("FileDescription", "Lentera"),
            ("FileVersion", version),
            ("InternalName", "lentera"),
            (
                "LegalCopyright",
                "Copyright (c) 2026 Lentera contributors. MIT License.",
            ),
            ("OriginalFilename", "lentera.exe"),
            ("ProductName", "Lentera"),
            ("ProductVersion", version),
        ]
        .into_iter()
        .map(|(key, value)| {
            let text = utf16z(value);
            node(key, 1, (text.len() / 2) as u16, &text, &[])
        })
        .collect();
        // Bahasa 0x0409 dengan halaman kode 1200 (Unicode).
        let table = node("040904B0", 1, 0, &[], &strings);
        let string_info = node("StringFileInfo", 1, 0, &[], &[table]);
        let translation = node("Translation", 0, 4, &[0x09, 0x04, 0xB0, 0x04], &[]);
        let var_info = node("VarFileInfo", 1, 0, &[], &[translation]);
        node(
            "VS_VERSION_INFO",
            0,
            fixed.len() as u16,
            &fixed,
            &[string_info, var_info],
        )
    }

    /// Satu blok VERSIONINFO: panjang, panjang nilai, tipe, kunci, nilai, anak.
    fn node(key: &str, kind: u16, value_len: u16, value: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![0; 6];
        out.extend(utf16z(key));
        pad(&mut out);
        out.extend_from_slice(value);
        for child in children {
            pad(&mut out);
            out.extend_from_slice(child);
        }
        let len = u16::try_from(out.len()).expect("VERSIONINFO terlalu besar");
        out[0..2].copy_from_slice(&len.to_le_bytes());
        out[2..4].copy_from_slice(&value_len.to_le_bytes());
        out[4..6].copy_from_slice(&kind.to_le_bytes());
        out
    }

    fn utf16z(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn pad(out: &mut Vec<u8>) {
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }

    fn word(data: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([data[at], data[at + 1]])
    }

    fn dword(data: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
    }
}
