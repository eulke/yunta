//! The crates the terminal application links, and the crates no other one does. What it
//! shows, what it says and the work it asks are `yunta-ui`'s, which reaches Yunta through
//! `yunta-api` and says every sentence in render's words, so the terminal application
//! names none of them, nor a second way to hold a terminal. And it is the one crate that
//! knows Ratatui and crossterm: an interface or a command that read keys or laid cells out
//! with them would be a second terminal application. The compiler holds the first; these
//! tests say so when a manifest is edited.

use std::path::{Path, PathBuf};

/// What the application reaches only through `yunta-ui`: Yunta's API and what stands below
/// it, and the words render says.
const THROUGH_THE_INTERFACE: [&str; 5] = [
    "yunta-api",
    "yunta-engine",
    "yunta-storage",
    "yunta-adapters",
    "yunta-render",
];

/// The CLI's way of holding a terminal, which the application does not share.
const THE_CLIS_TERMINAL: [&str; 2] = ["console", "indicatif"];

/// The crates that lay a screen out in cells and read its keys: this crate's alone.
const ITS_OWN: [&str; 2] = ["ratatui", "crossterm"];

#[test]
fn the_terminal_application_names_neither_the_api_render_nor_the_clis_terminal() {
    let named = dependencies(include_str!("../Cargo.toml"));
    let through: Vec<&str> = THROUGH_THE_INTERFACE
        .into_iter()
        .filter(|krate| named.contains(krate))
        .collect();
    assert!(
        through.is_empty(),
        "yunta-tui must not depend on {through:?}: what a screen shows, says and asks is \
         yunta-ui's, which reaches Yunta through yunta-api and words it in render's words, so \
         the desktop gets it too. It depends on: {named:?}"
    );
    let terminal: Vec<&str> = THE_CLIS_TERMINAL
        .into_iter()
        .filter(|krate| named.contains(krate))
        .collect();
    assert!(
        terminal.is_empty(),
        "yunta-tui must not depend on {terminal:?}: those hold the CLI's prompts; the \
         application holds its terminal with crossterm. It depends on: {named:?}"
    );
}

#[test]
fn no_other_crate_names_ratatui_or_crossterm() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the workspace's root, two above the crate");
    let mut named = Vec::new();
    for manifest in manifests(root) {
        let text = std::fs::read_to_string(&manifest).expect("a manifest that reads");
        for krate in ITS_OWN {
            if mentions(&text, krate) {
                named.push(format!("{} names {krate}", manifest.display()));
            }
        }
    }
    assert!(
        named.is_empty(),
        "only yunta-tui may name Ratatui or crossterm: laying cells out and reading keys \
         belongs to the terminal application, so an interface reads strokes from yunta-ui \
         and a command keeps the CLI's own prompts. {named:#?}"
    );
}

/// The manifest of every crate in the workspace but this one: each crate under `crates/`,
/// and `xtask`. The workspace's own table holds the versions every crate takes from it, so
/// it is not among them.
fn manifests(root: &Path) -> Vec<PathBuf> {
    let crates = std::fs::read_dir(root.join("crates")).expect("the workspace's crates");
    let mut found: Vec<PathBuf> = crates
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|dir| dir.file_name().is_some_and(|name| name != "tui"))
        .map(|dir| dir.join("Cargo.toml"))
        .filter(|manifest| manifest.exists())
        .collect();
    found.push(root.join("xtask/Cargo.toml"));
    found.sort();
    found
}

/// Whether `manifest` names `krate` in any of its tables: as a key, or as a table of its
/// own.
fn mentions(manifest: &str, krate: &str) -> bool {
    manifest.lines().map(str::trim).any(|line| {
        let key = line.split_once('=').map(|(key, _)| key.trim());
        key.is_some_and(|key| key == krate || key == format!("{krate}.workspace"))
            || line.ends_with(&format!(".{krate}]"))
    })
}

/// The crate names in the manifest's `[dependencies]` table: the key left of the first `=`
/// on every line of that one section, comments and blank lines skipped.
fn dependencies(manifest: &str) -> Vec<&str> {
    manifest
        .lines()
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, _)| key.trim().trim_end_matches(".workspace"))
        .collect()
}
