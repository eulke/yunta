//! The crates the interface never links. What a person has open, what a
//! stroke does to it and what it draws are the same on a terminal and in a
//! window, so the crates that hold a terminal belong to the surface that lays
//! the interface out, and the engine, the store and the adapters are reached
//! through `yunta-api`, as every interface reaches them. The compiler holds
//! it; this test says so when the manifest is edited.

/// The crates that hold a terminal: a surface's to name.
const A_TERMINAL: [&str; 5] = ["crossterm", "console", "nix", "indicatif", "ratatui"];

/// The crates below the API, which an interface reaches only through it.
const BELOW_THE_API: [&str; 3] = ["yunta-engine", "yunta-storage", "yunta-adapters"];

#[test]
fn the_interface_never_links_a_terminal_the_engine_the_store_or_the_adapters() {
    let named = dependencies(include_str!("../Cargo.toml"));
    let terminal: Vec<&str> = A_TERMINAL
        .into_iter()
        .filter(|krate| named.contains(krate))
        .collect();
    assert!(
        terminal.is_empty(),
        "yunta-ui must not depend on {terminal:?}: reading keys and laying a screen out \
         belong to the surface that shows the interface, the terminal application or the \
         desktop, so the interface stays the same in both. It depends on: {named:?}"
    );
    let below: Vec<&str> = BELOW_THE_API
        .into_iter()
        .filter(|krate| named.contains(krate))
        .collect();
    assert!(
        below.is_empty(),
        "yunta-ui must not depend on {below:?}: the work a stage asks for, its performer \
         asks yunta-api, which reaches the engine, the store and the adapters for every \
         interface alike. It depends on: {named:?}"
    );
}

/// The crate names in the manifest's `[dependencies]` table: the key left of
/// the first `=` on every line of that one section, comments and blank lines
/// skipped.
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
