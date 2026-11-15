use sha2::{Digest, Sha256};
use std::{env, fmt::Write, fs, path::Path};

fn collect(root: &Path, directory: &Path, files: &mut Vec<(String, std::path::PathBuf)>) {
    for entry in fs::read_dir(directory).expect("read built-in Web directory") {
        let entry = entry.expect("read Web entry");
        let kind = entry.file_type().expect("inspect Web entry");
        assert!(
            !kind.is_symlink(),
            "built-in Web assets must be ordinary files"
        );
        if kind.is_dir() {
            collect(root, &entry.path(), files);
        } else if kind.is_file() {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("Web root")
                .to_str()
                .expect("UTF-8 Web path")
                .replace('\\', "/");
            files.push((relative, path));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=RPI_WORK_BUILTIN_WEB_DIR");
    let mut files = Vec::new();
    if let Some(root) = env::var_os("RPI_WORK_BUILTIN_WEB_DIR") {
        let root = fs::canonicalize(root).expect("built-in Work Web bundle must exist");
        assert!(
            root.join("index.html").is_file(),
            "built-in Work Web index missing"
        );
        println!("cargo:rerun-if-changed={}", root.display());
        collect(&root, &root, &mut files);
        files.sort_by(|left, right| left.0.cmp(&right.0));
    }
    let mut digest = Sha256::new();
    let mut table = String::from("pub(super) static FILES: &[(&str, &[u8])] = &[\n");
    for (relative, path) in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).expect("read built-in Web file");
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(&bytes);
        writeln!(
            table,
            "({relative:?}, include_bytes!({:?})),",
            path.to_str().expect("UTF-8 build path")
        )
        .expect("write Web table row");
    }
    table.push_str("];\n");
    writeln!(
        table,
        "pub(super) const ID: &str = \"{:x}\";",
        digest.finalize()
    )
    .expect("write Web asset digest");
    fs::write(
        Path::new(&env::var_os("OUT_DIR").expect("Cargo OUT_DIR")).join("web_assets.rs"),
        table,
    )
    .expect("write built-in Web asset table");
}
