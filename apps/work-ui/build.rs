fn main() {
    println!("cargo:rerun-if-changed=branding.rc");
    println!("cargo:rerun-if-changed=../../assets/branding/rpi.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for("branding.rc", ["work-ui"], embed_resource::NONE)
            .manifest_required()
            .expect("embed the canonical Web brand into the Windows executable");
    }
}
