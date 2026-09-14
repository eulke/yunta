// The engine knows an adapter only by the port: the trait it calls, the
// request it hands over, the events it reads back. That port is
// `yunta_core::port`, so `yunta-adapters` — where `claude-code`, `codex`
// and `mock` are actually built — is a crate the engine's library never
// links, and the compiler is what holds that. This test is what says so
// when the manifest is edited.
//
// The library, not the tests: a port is exercised against an
// implementation of it, so `[dev-dependencies]` does name the crate and
// the engine's own tests run whole workflows on the mock. What would
// break the frontier is a line in `[dependencies]`, which is the table
// this reads.

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
