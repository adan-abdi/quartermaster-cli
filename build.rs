use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let static_dir = manifest_dir.join("static");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("missing out dir"));

    println!("cargo:rerun-if-changed={}", static_dir.display());

    let mut files = Vec::new();
    if static_dir.exists() {
        collect_files(&static_dir, &static_dir, &mut files)
            .expect("failed to collect static files");
    }

    files.sort();

    let mut generated = String::from("static EMBEDDED_ASSETS: &[EmbeddedAsset] = &[\n");
    for relative_path in files {
        generated.push_str(&format!(
            "    EmbeddedAsset {{ path: \"/{path}\", bytes: include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/static/{path}\")) }},\n",
            path = relative_path
        ));
    }
    generated.push_str("];\n");

    fs::write(out_dir.join("embedded_assets.rs"), generated)
        .expect("failed to write embedded asset registry");
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_files(root, &path, files)?;
            continue;
        }

        let relative_path = path
            .strip_prefix(root)
            .expect("static file should stay within static directory");
        files.push(normalize_path(relative_path));
    }

    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
