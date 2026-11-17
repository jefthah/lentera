fn main() {
    // Gaya gelap untuk kolom teks, daftar, dan pilihan bawaan Slint; sisanya
    // digambar sendiri di ui/app.slint.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("UI Slint gagal dikompilasi");

    // Manifest Windows: DPI per monitor dan tampilan dialog modern.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("assets/app.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
        println!("cargo:rerun-if-changed=assets/app.manifest");
    }
}
