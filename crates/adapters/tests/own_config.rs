//! The configuration trees each agent CLI owns, which no session of a run
//! writes: a file under a tree is covered by the tree's own glob.

use yunta_adapters::{ClaudeCodeAdapter, CodexAdapter};
use yunta_core::port::Adapter;
use yunta_core::AdapterSettings;

fn claude() -> ClaudeCodeAdapter {
    ClaudeCodeAdapter::new(&AdapterSettings {
        adapter_settings: None,
        binary: Some(yunta_testkit_core::stubs::claude_code()),
    })
}

fn codex() -> CodexAdapter {
    CodexAdapter::new(&AdapterSettings {
        adapter_settings: None,
        binary: Some(yunta_testkit_core::stubs::codex()),
    })
}

#[test]
fn the_claude_config_trees_cover_every_file_under_them() {
    let trees = yunta_core::scope_globset(&claude().own_config()).unwrap();
    assert!(trees.is_match(".claude/settings.json"));
    assert!(trees.is_match(".claude/settings.local.json"));
    assert!(trees.is_match(".claude/hooks/guard.sh"));
    assert!(trees.is_match(".mcp.json"));
}

#[test]
fn the_codex_config_tree_covers_every_file_under_it() {
    let trees = yunta_core::scope_globset(&codex().own_config()).unwrap();
    assert!(trees.is_match(".codex/config.toml"));
    assert!(trees.is_match(".codex/state/cache.json"));
}

/// Every tree an adapter declares as its own is one every run protects, and
/// every tree a run protects as an agent's is some adapter's: a new adapter
/// whose trees the protected set does not hold fails here.
#[test]
fn every_run_protects_exactly_the_trees_the_adapters_declare() {
    let mut declared: Vec<String> = claude()
        .own_config()
        .into_iter()
        .chain(codex().own_config())
        .map(|glob| glob.as_str().to_string())
        .collect();
    declared.sort();
    let mut protected: Vec<String> = yunta_core::protected::AGENT_CONFIG
        .iter()
        .map(|glob| glob.to_string())
        .collect();
    protected.sort();
    assert_eq!(declared, protected);
}
