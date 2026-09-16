//! Every name the engine writes under a run directory is written down
//! once, in `run_dir`, and every other site asks for it.
//!
//! A grep test, because the defect it guards against is a string: a
//! second `run_dir.join("manifest.yaml")` compiles, runs, and disagrees
//! with the first the day one of them moves. The layout of a run
//! directory is a convention, and a convention this system keeps has one
//! home.

use std::path::Path;

/// The names `run_dir` owns, and the crates that must not spell them
/// themselves.
const OWNED: [&str; 5] = [
    "\"manifest.yaml\"",
    "\"progress.md\"",
    "\"task-worktrees\"",
    "join(\"sessions\")",
    "join(\"artifacts\")",
];

/// Where `run_dir` itself lives — the one file allowed to hold them.
const HOME: &str = "src/run_dir.rs";

fn sources(crate_dir: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut stack = vec![crate_dir.join("src")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let relative = path
                    .strip_prefix(crate_dir)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                if relative.replace('\\', "/") == HOME {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    found.push((relative, text));
                }
            }
        }
    }
    found
}

#[test]
fn every_path_under_the_run_dir_is_named_once() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = [
        root.to_path_buf(),
        root.join("../cli"),
        root.join("../core"),
    ];
    let mut spelled = Vec::new();
    for crate_dir in crates {
        for (file, text) in sources(&crate_dir) {
            for name in OWNED {
                if text.contains(name) {
                    spelled.push(format!("{file} spells {name}"));
                }
            }
        }
    }
    assert!(
        spelled.is_empty(),
        "the run directory's layout lives in `engine/{HOME}` and nowhere else — ask it for the \
         path instead of writing the name:\n  {}",
        spelled.join("\n  ")
    );
}
