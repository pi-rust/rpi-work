fn main() {
    println!("cargo:rerun-if-changed=src/browser_mac.m");
    println!("cargo:rerun-if-changed=src/qr_mac.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let mut compiler = cc::Build::new();
        compiler
            .files(["src/browser_mac.m", "src/qr_mac.m"])
            .flag("-fobjc-arc");
        if std::env::var_os("MACOSX_DEPLOYMENT_TARGET").is_none() {
            compiler.flag("-mmacosx-version-min=11.0");
        }
        compiler.compile("work_gpui_browser");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=CoreImage");
        println!("cargo:rustc-link-lib=framework=WebKit");
    }
}
