//! Logs an earlier binary wrote, checked in as they were recorded: each one
//! replays with no broken state, and the pause one of them recorded reads
//! back as the sentence it stored.

use yunta_core::events::{PauseReason, StoredEvent};
use yunta_engine::derive;

/// A log an earlier binary wrote replays: no kind or payload it holds stops
/// reading as the binary that wrote it meant.
/// Reached by: `crates/core/src/events/**`, `crates/engine/src/replay.rs`,
/// `crates/engine/src/replay/**`, `crates/core/src/best_effort.rs`,
/// `crates/core/src/capabilities.rs`, `crates/core/src/config/**`,
/// `crates/core/src/confinement.rs`, `crates/core/src/diagnostic/**`,
/// `crates/core/src/expansion.rs`, `crates/core/src/fence.rs`, `crates/core/src/findings/**`,
/// `crates/core/src/glob.rs`, `crates/core/src/hash.rs`, `crates/core/src/ids.rs`,
/// `crates/core/src/ids/**`, `crates/core/src/inputs.rs`, `crates/core/src/policy.rs`,
/// `crates/core/src/port/**`, `crates/core/src/protected.rs`, `crates/core/src/protected/**`,
/// `crates/core/src/rail/**`, `crates/core/src/reserved.rs`, `crates/core/src/shown.rs`,
/// `crates/core/src/template.rs`, `crates/core/src/workflow/**`
#[test]
fn checked_in_logs_replay_to_what_they_recorded() {
    let load = |name: &str| -> Vec<StoredEvent> {
        let path = format!(
            "{}/tests/fixtures/logs/{name}.jsonl",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    for name in [
        "pause-sentence",
        "policy-sentence",
        "reroute-cause",
        "outcome-sentence",
        "scope-request",
        "codex-write-refused",
        "claude-run",
    ] {
        assert_eq!(derive(&load(name)).broken, None, "{name}");
    }
    let state = derive(&load("pause-sentence"));
    assert_eq!(
        state.run.paused().map(|(pause, _)| pause),
        Some(&PauseReason::Recorded {
            sentence: "node `grill` failed: Not logged in · Please run /login".into(),
        })
    );
}
