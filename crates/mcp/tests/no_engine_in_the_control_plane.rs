//! The crates the control plane never links. Each tool asks `yunta-api`
//! what the command a person runs asks it, and the API reaches the engine,
//! the store and the adapters for every interface alike, so none of them is
//! the control plane's to name. The compiler holds it; this test says so
//! when the manifest is edited.

/// The crates below the API, which an interface reaches only through it.
const BELOW_THE_API: [&str; 3] = ["yunta-engine", "yunta-storage", "yunta-adapters"];

#[test]
fn the_control_plane_never_links_the_engine_the_store_or_the_adapters() {
    let named = dependencies(include_str!("../Cargo.toml"));
    let below: Vec<&str> = BELOW_THE_API
        .into_iter()
        .filter(|krate| named.contains(krate))
        .collect();
    assert!(
        below.is_empty(),
        "yunta-mcp must not depend on {below:?}: what a tool reads, starts and answers, it \
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
