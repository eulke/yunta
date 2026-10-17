//! What an adapter knows of the shell it runs under arrives as values the
//! shell captured; no adapter module reads the process environment, so a
//! test hands it what it chose and nothing undeclared reaches a probe.

use std::path::Path;

/// The call that reads the process environment, spelled so this file does
/// not read as one.
const READS_THE_PROCESS: &str = concat!("std::env::", "var");

fn sources(dir: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|kind| kind == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            for (index, line) in text.lines().enumerate() {
                let code = line.trim_start();
                if !code.starts_with("//") && code.contains(READS_THE_PROCESS) {
                    found.push(format!("{}:{}", path.display(), index + 1));
                }
            }
        }
    }
}

#[test]
fn no_adapter_module_reads_the_process_environment() {
    let mut found = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    assert!(
        found.is_empty(),
        "the shell's environment arrives captured, through `Env`:\n  {}",
        found.join("\n  ")
    );
}
