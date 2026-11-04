//! The crates the engine's library never links. It knows an adapter only by
//! the port: the trait it calls, the request it hands over, the events it
//! reads back. That port is `yunta_core::port`, so `yunta-adapters` — where
//! `claude-code`, `codex` and `mock` are actually built — stays out. It never
//! draws either, so `yunta-render`, which the CLI and the adapters draw with,
//! stays out too. The compiler holds both; these tests say so when the
//! manifest is edited.
//
// The library, not the tests: a port is exercised against an
// implementation of it and a receipt is read the way a person is shown
// it, so `[dev-dependencies]` does name both crates. What would break the
// frontier is a line in `[dependencies]`, which is the table this reads.

#[test]
fn the_engine_library_never_links_the_crate_that_builds_concrete_adapters() {
    let named = dependencies(include_str!("../Cargo.toml"));
    assert!(
        !named.contains(&"yunta-adapters"),
        "yunta-engine must not depend on yunta-adapters — the port it calls is \
         `yunta_core::port`, and a concrete adapter reaches the engine as a \
         `dyn Adapter` the CLI constructed. It depends on: {named:?}"
    );
}

/// The engine decides and records; it never draws. It hands the CLI and the
/// adapters core data, and they draw it with `yunta-render`, so a surface can
/// change what a person reads without the engine knowing.
#[test]
fn the_engine_library_never_links_the_crate_that_draws() {
    let named = dependencies(include_str!("../Cargo.toml"));
    assert!(
        !named.contains(&"yunta-render"),
        "yunta-engine must not depend on yunta-render — the engine hands core \
         data, and the CLI and the adapters draw it. It depends on: {named:?}"
    );
}

/// The crate names in the manifest's `[dependencies]` table: the key
/// left of the first `=` on every line of that one section, comments and
/// blank lines skipped.
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
