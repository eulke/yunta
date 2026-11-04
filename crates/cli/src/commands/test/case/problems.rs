//! The problems a case's run is held to beyond its own expectations: a
//! fixture that describes sessions which never ran, and where the run and
//! each node stand when it ends.

use std::path::Path;
use std::sync::Arc;

use surco::words::Standing;
use yunta_engine::RunFrame;

use super::{terminal_label, Played, TestCase};

/// A fixture that describes sessions which never happened describes a run
/// nobody made: the case fails, always, and names which scripts went
/// unclaimed. There is no expectation to opt into — the fixture is the
/// case's own account of what the run does.
pub(super) fn unclaimed_problems(
    fixture_path: &Path,
    script: Option<&Arc<yunta_adapters::MockAdapter>>,
) -> Vec<String> {
    let unclaimed = script.map(|mock| mock.unconsumed()).unwrap_or_default();
    if unclaimed.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "fixture `{}`: {} nothing opened: {}",
        fixture_path.display(),
        surco::text::counted(unclaimed.len(), "scripted session"),
        unclaimed
            .iter()
            .map(|index| format!("#{}", index + 1))
            .collect::<Vec<_>>()
            .join(", ")
    )]
}

/// Where the run and each node stand is read off the run's frame, the
/// one place that tells a node the run's mode leaves out from one it
/// has not reached and a run that finished from one findings
/// block — so a case says what `status` says. The report's state is
/// the log as the run left it, read at its last event.
pub(super) fn standing_problems(case: &TestCase, played: &Played) -> Vec<String> {
    let state = &played.report.state;
    let read_at = state.clock().last_at().unwrap_or_default();
    // The case's run ended in this process, which drove it.
    let frame = yunta_engine::run_frame_of(
        &played.prepared.run_id,
        &played.manifest.workflow,
        state,
        None,
        &yunta_engine::Engine::this(read_at),
        read_at,
    );
    let mut problems = Vec::new();
    let expected_word = case.expect.final_state.word();
    let reached = frame.standing;
    if reached != expected_word {
        problems.push(format!(
            "final_state: expected {expected_word}, got {}",
            terminal_label(reached, &played.report.terminal)
        ));
    }
    for (node_id, expected) in &case.expect.nodes {
        problems.extend(node_problems(&frame, node_id, expected));
    }
    problems
}

/// What a case expected of `node_id` and the run's frame does not hold.
///
/// A node that fans out to several runners runs as one node per runner
/// (`review@reviewer`, ...): the id the workflow declares names all of
/// them, and each has to stand where the case says.
fn node_problems(frame: &RunFrame, node_id: &str, expected: &str) -> Vec<String> {
    let named: Vec<_> = frame
        .nodes
        .iter()
        .filter(|node| {
            node.id == node_id
                || node
                    .id
                    .as_str()
                    .strip_prefix(node_id)
                    .is_some_and(|runner| runner.starts_with('@'))
        })
        .collect();
    if named.is_empty() {
        return vec![format!(
            "node {node_id}: expected {expected}, and the workflow declares no such node"
        )];
    }
    named
        .into_iter()
        .filter(|node| node.standing.word() != expected)
        .map(|node| {
            format!(
                "node {}: expected {expected}, got {}{}",
                node.id,
                node.standing,
                renamed(expected, node.standing)
            )
        })
        .collect()
}

/// Why a node expected in a word the vocabulary moved stands at another:
/// a node waiting on a person is `needs you` now, and `waiting` is a
/// clock's alone.
fn renamed(expected: &str, got: Standing) -> String {
    let said = match (expected, got) {
        ("waiting", Standing::NeedsYou) => Some(format!(
            "a node waiting on a person is `{}` now; `waiting` is a clock's",
            Standing::NeedsYou.word()
        )),
        _ => super::retired(expected),
    };
    said.map(|said| format!(" ({said})")).unwrap_or_default()
}
