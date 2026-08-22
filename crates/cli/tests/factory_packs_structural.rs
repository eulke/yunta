//! Factory packs must stay "borrables sin afectar capacidades del engine" —
//! none of `yunta/starter`/`yunta/fragua`'s names appear anywhere in the
//! engine's own source, since a workspace crate special-casing a pack
//! by name would be exactly the kind of embedded workflow the engine
//! must never depend on. Pure text scan, no build or run involved.

use std::path::Path;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn rust_files_under(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_crate_source_mentions_either_factory_pack_by_name() {
    let root = repo_root();
    let mut files = Vec::new();
    for crate_name in ["core", "storage", "adapters", "engine", "cli"] {
        rust_files_under(
            &root.join("crates").join(crate_name).join("src"),
            &mut files,
        );
    }
    assert!(!files.is_empty(), "expected to find workspace source files");

    let needles = [
        "yunta/starter",
        "yunta/fragua",
        "packs/starter",
        "packs/fragua",
    ];
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for needle in needles {
            assert!(
                !text.contains(needle),
                "{} references `{needle}` — the engine must grant factory packs no special \
                 treatment",
                file.display()
            );
        }
    }
}
