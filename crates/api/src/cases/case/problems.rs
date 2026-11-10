//! The problems a case's run is held to beyond its own expectations: a
//! fixture that describes sessions which never ran, and where the run and
//! each node stand when it ends.

use std::path::Path;
use std::sync::Arc;

use yunta_render::page::CaseProblem;
use yunta_replay::frame::RunFrame;

use super::{stopped_by, Played, TestCase};

/// A fixture that describes sessions which never happened describes a run
/// nobody made: the case fails, always, and names which scripts went
/// unclaimed. There is no expectation to opt into — the fixture is the
/// case's own account of what the run does.
pub(super) fn unclaimed_problems(
    fixture_path: &Path,
    script: Option<&Arc<yunta_adapters::MockAdapter>>,
) -> Vec<CaseProblem> {
    let unclaimed = script.map(|mock| mock.unconsumed()).unwrap_or_default();
    if unclaimed.is_empty() {
        return Vec::new();
    }
    vec![CaseProblem::Unclaimed {
        fixture: fixture_path.to_path_buf(),
        sessions: unclaimed.iter().map(|index| index + 1).collect(),
    }]
}

/// Where the run and each node stand is read off the run's frame, the
/// one place that tells a node the run's mode leaves out from one it
/// has not reached and a run that finished from one findings
/// block — so a case says what `status` says. The report's state is
/// the log as the run left it, read at its last event.
pub(super) fn standing_problems(case: &TestCase, played: &Played) -> Vec<CaseProblem> {
    let state = &played.report.state;
    let read_at = state.clock().last_at().unwrap_or_default();
    // The case's run ended in this process, which drove it.
    let frame = yunta_replay::frame::RunFrame::of(
        &played.run_id,
        &played.manifest.workflow,
        state,
        None,
        &yunta_replay::registry::Engine::this(yunta_core::Pid::current()),
        read_at,
    );
    let mut problems = Vec::new();
    let expected = case.expect.final_state.word();
    if frame.standing != expected {
        problems.push(CaseProblem::FinalState {
            expected,
            got: frame.standing,
            stopped_by: stopped_by(&played.report.terminal),
        });
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
fn node_problems(frame: &RunFrame, node_id: &str, expected: &str) -> Vec<CaseProblem> {
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
        return vec![CaseProblem::NoSuchNode {
            node: node_id.to_string(),
            expected: expected.to_string(),
        }];
    }
    named
        .into_iter()
        .filter(|node| node.standing.word() != expected)
        .map(|node| CaseProblem::Node {
            node: node.id.clone(),
            expected: expected.to_string(),
            got: node.standing,
        })
        .collect()
}
