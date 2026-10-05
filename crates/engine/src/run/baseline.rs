//! The measurement a lineage takes once: what worked on the tree before
//! the invocation started.
//!
//! A run measures on its first wake, before any node of its own can
//! change the tree, only when its config names a suite, and only when
//! something of its own reads the measurement: a `baseline_compare`, a
//! loop whose tasks are held to the suite, or a composed run that may
//! hold either — a run that reads nothing pays for nothing. A run born
//! of another — a `kind: workflow` child, a promotion successor — is
//! born holding the root's measurement instead, so every
//! `baseline_compare` anywhere in the lineage compares against the same
//! reading: a regression a parent introduced is a regression its child
//! sees. What the suite wrote stays with the run that ran it; what a
//! descendant carries is the fact, naming the run that holds the bytes.

use std::path::Path;

use yunta_core::events::{
    BaselineCapturedPayload, BaselineOrigin, BaselineResults, EventPayload, RunEvent,
};
use yunta_core::{ContentHash, RunId};

use super::{RunCtx, RunError};
use crate::replay::RunState;

/// Whether a run of `workflow` reads its lineage's measurement: a node
/// compares against it, a loop holds its tasks to it, or a composed run
/// — whose workflow is resolved only when it is born — may do either.
pub fn reads_the_baseline(workflow: &yunta_core::Workflow) -> bool {
    workflow.nodes.iter().any(reads_the_measurement)
}

/// Whether running `node` reads the lineage's measurement: it compares
/// against it, holds tasks to it, hands it to a run it gives birth to —
/// whose workflow is resolved only then — or puts a plan in front of a
/// person, whose every task the suite holds. A group reads it when any
/// node it runs does.
///
/// The one reading of what reads the measurement: what makes a run
/// measure at all, and what waits for a measurement taken aside.
/// Whether running `node` waits for the measurement: what reads it, but a
/// loop, whose tasks wait for it only where one of their guards is judged
/// — a check whose own criteria pass. Its tasks start beside it.
pub fn waits_for_the_measurement(node: &yunta_core::Node) -> bool {
    match &node.kind {
        yunta_core::NodeKind::Loop { .. } => false,
        yunta_core::NodeKind::Parallel { nodes, .. } => nodes.iter().any(waits_for_the_measurement),
        _ => reads_the_measurement(node),
    }
}

pub fn reads_the_measurement(node: &yunta_core::Node) -> bool {
    use yunta_core::{ArtifactKind, ArtifactRefId, CheckBuiltin, NodeKind};
    match &node.kind {
        NodeKind::Check(CheckBuiltin::BaselineCompare)
        | NodeKind::Loop { .. }
        | NodeKind::Workflow { .. } => true,
        NodeKind::Gate { shows, .. } => shows.iter().any(|shown| {
            shown.id
                == ArtifactRefId::Kind {
                    kind: ArtifactKind::Tasks,
                }
        }),
        NodeKind::Parallel { nodes, .. } => nodes.iter().any(reads_the_measurement),
        _ => false,
    }
}

/// The measurement a run hands to one it gives birth to: the fact,
/// named by the run that took it.
///
/// It carries no bytes. The suite's output stays under the `baseline/`
/// of the run that measured, which `measured_by` names — a lineage of
/// any depth keeps one copy, and a birth stays a write of facts.
#[derive(Debug, Clone, PartialEq)]
pub struct BirthBaseline {
    /// The root that ran the suite, never the parent this was handed
    /// down through.
    pub measured_by: RunId,
    pub command: String,
    pub results: BaselineResults,
    pub hash: ContentHash,
}

impl BirthBaseline {
    /// The fact as the run receiving it records it.
    pub(super) fn captured(&self) -> BaselineCapturedPayload {
        BaselineCapturedPayload {
            command: self.command.clone(),
            results: self.results.clone(),
            hash: self.hash.clone(),
            origin: BaselineOrigin::Inherited {
                run: self.measured_by.clone(),
            },
            tree: None,
            duration_ms: None,
        }
    }
}

/// What `from` hands to a run it gives birth to, with the root
/// resolved: `from` itself when it measured, and the root it names when
/// it was born holding one. `None` for a lineage whose root declared no
/// suite.
///
/// Pure: a function of the state the giving run's log derives.
pub fn inherited(from: &RunId, from_state: &RunState) -> Option<BirthBaseline> {
    let held = from_state.run.baseline()?;
    Some(BirthBaseline {
        measured_by: match &held.origin {
            BaselineOrigin::Measured => from.clone(),
            BaselineOrigin::Inherited { run } => run.clone(),
        },
        command: held.command.clone(),
        results: held.results.clone(),
        hash: held.hash.clone(),
    })
}

/// Measures the suite the scheduler decided this run owes: runs it on
/// the tree the run opens on, under the run's own supervision, keeps
/// everything it wrote under `baseline/`, and records the measurement.
///
/// A suite the cancellation stops records nothing and answers `Ok(())`:
/// a step is not a node, so there is no node end to write, and the
/// loop's next turn sees the token fired and pauses the run. The next
/// invocation finds no measurement on the log and measures then.
pub(super) async fn measure(ctx: &RunCtx<'_>, suite: String) -> Result<(), RunError> {
    measure_in(ctx, suite, ctx.worktree, ctx.root_supervision()).await
}

/// [`measure`], with the suite run in `cwd` under `supervision`.
pub(super) async fn measure_in(
    ctx: &RunCtx<'_>,
    suite: String,
    cwd: &Path,
    supervision: crate::process::Supervision<'_>,
) -> Result<(), RunError> {
    // Read before the suite runs: what it measured is the tree as it
    // found it, whatever the run leaves behind in it. Only a shortcut
    // hangs on it — without it the first check runs the suite itself —
    // so a tree that could not be read, a cancelled one among them,
    // costs that and nothing else.
    let measured_on = ctx.memo.tree_for(&suite, cwd, supervision).await.ok();
    let started = std::time::Instant::now();
    let output = match super::check_exec::run_command(supervision, cwd, &suite).await? {
        super::check_exec::CommandRun::Done(output) => output,
        super::check_exec::CommandRun::Cancelled => return Ok(()),
    };
    let took = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    // Every task is held to the suite as a guard, and the first of them
    // starts from the tree just measured: a green measurement is its
    // answer there, so the suite is not run twice on one tree.
    if let Some(tree) = measured_on.as_ref().filter(|_| output.exit_code == 0) {
        ctx.memo.passed_on(&suite, tree);
    }

    keep_capture(ctx.run_dir, output.stdout.as_bytes()).await?;
    let command = suite.clone();
    ctx.emit(
        None,
        EventPayload::Run(RunEvent::BaselineCaptured(BaselineCapturedPayload {
            command: suite,
            results: BaselineResults {
                exit_code: output.exit_code,
                summary: summary(&output.stdout),
            },
            hash: yunta_core::sha256_hex(output.stdout.as_bytes()),
            origin: BaselineOrigin::Measured,
            tree: measured_on.map(|tree| tree.content().clone()),
            duration_ms: Some(took),
        })),
    )
    .await?;
    // The checks of a loop that started beside the measurement wait for
    // this answer.
    ctx.memo.suite().settle(&command, output.exit_code == 0);
    Ok(())
}

/// Keeps everything the suite wrote under the measuring run's
/// `baseline/`, which the hash on the log names. Only a run that
/// measured has one: a run born holding a measurement reads the bytes
/// under the run its origin names.
pub(super) async fn keep_capture(run_dir: &Path, output: &[u8]) -> Result<(), RunError> {
    let dir = crate::run_dir::baseline_dir(run_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|source| RunError::Io {
            context: format!("create run directory `{}`", dir.display()),
            source,
        })?;
    let capture = crate::run_dir::baseline_capture(run_dir);
    tokio::fs::write(&capture, output)
        .await
        .map_err(|source| RunError::Io {
            context: format!("write `{}`", capture.display()),
            source,
        })
}

/// The tail of the suite's output a reader sees without opening what the
/// measuring run kept.
pub(super) fn summary(stdout: &str) -> String {
    stdout.lines().rev().take(5).collect::<Vec<_>>().join("\n")
}
