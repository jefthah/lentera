fn main() {
    // Gaya gelap untuk kolom teks, daftar, dan pilihan bawaan Slint; sisanya
    // digambar sendiri di ui/app.slint.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("UI Slint gagal dikompilasi");
}
