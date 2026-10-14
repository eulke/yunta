//! The problems a case's run is held to beyond its own expectations: a
//! fixture that describes sessions which never ran, and where the run and
//! each node stand when it ends.

use std::path::Path;
use std::sync::Arc;

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
        "fixture `{}`: {} nothing opened — {}",
        fixture_path.display(),
        yunta_core::text::counted(unclaimed.len(), "scripted session"),
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
    let frame = yunta_engine::run_frame_of(
        &played.prepared.run_id,
        &played.manifest.workflow,
        state,
        None,
        read_at,
    );
    let mut problems = Vec::new();
    let expected_word = case.expect.final_state.word();
    let reached = crate::render::run_word(&frame);
    if reached != expected_word {
        problems.push(format!(
            "final_state: expected {expected_word}, got {}",
            terminal_label(reached, &played.report.terminal)
        ));
    }
    for (node_id, expected) in &case.expect.nodes {
        // A node that fans out to several runners runs as one node per
        // runner (`review@reviewer`, ...): the id the workflow declares
        // names all of them, and each has to stand where the case says.
        let named: Vec<_> = frame
            .nodes
            .iter()
            .filter(|node| {
                node.id == node_id.as_str()
                    || node
                        .id
                        .as_str()
                        .strip_prefix(node_id.as_str())
                        .is_some_and(|runner| runner.starts_with('@'))
            })
            .collect();
        if named.is_empty() {
            problems.push(format!(
                "node {node_id}: expected {expected}, and the workflow declares no such node"
            ));
        }
        for node in named {
            let got = crate::render::standing(&node.state).word.word();
            if got != expected {
                problems.push(format!("node {}: expected {expected}, got {got}", node.id));
            }
        }
    }
    problems
}
