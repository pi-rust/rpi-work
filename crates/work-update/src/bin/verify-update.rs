#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), String> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or("Usage: verify-update <release-directory>")?;
    let bytes = std::fs::read(root.join("update.json")).map_err(|e| e.to_string())?;
    let signature =
        std::fs::read_to_string(root.join("update.json.sig")).map_err(|e| e.to_string())?;
    let manifest = work_update::verify_manifest(&bytes, &signature, work_update::PUBLIC_KEY)?;
    for (target, artifact) in &manifest.artifacts {
        let release = work_update::Release {
            manifest: manifest.clone(),
            target: target.clone(),
            artifact: artifact.clone(),
        };
        work_update::native::verify_download(&root.join(&artifact.name), &release)?;
    }
    println!(
        "Verified signed release {}: {} packages",
        manifest.version,
        manifest.artifacts.len()
    );
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
