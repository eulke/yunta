//! The crates the CLI never links. A command reads a run, drives it, answers
//! it and ends it through `yunta-api`, which reaches the engine, the store and
//! the adapters for every interface alike, so none of them is the CLI's to
//! name. The compiler holds it; this test says so when the manifest is
//! edited.
//
// The binary, not the tests: a test that stages what a command reads drives
// the engine or the store directly, so `[dev-dependencies]` does name them.
// What would break the frontier is a line in `[dependencies]`, which is the
// table this reads.

/// The crates below the API, which an interface reaches only through it.
const BELOW_THE_API: [&str; 3] = ["yunta-engine", "yunta-storage", "yunta-adapters"];

#[test]
fn the_cli_never_links_the_engine_the_store_or_the_adapters() {
    let named = dependencies(include_str!("../Cargo.toml"));
    let below: Vec<&str> = BELOW_THE_API
        .into_iter()
        .filter(|krate| named.contains(krate))
        .collect();
    assert!(
        below.is_empty(),
        "yunta must not depend on {below:?}: what a command reads, drives, answers and \
         ends, it asks yunta-api, which reaches the engine, the store and the adapters for \
         every interface alike. It depends on: {named:?}"
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
