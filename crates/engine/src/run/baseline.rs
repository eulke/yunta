//! The measurement a lineage takes once: what worked on the tree before
//! the invocation started.
//!
//! A run measures on its first wake, before any node of its own can
//! change the tree, and only when its config names a suite. A run born
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
    let output =
        match super::check_exec::run_command(ctx.root_supervision(), ctx.worktree, &suite).await? {
            super::check_exec::CommandRun::Done(output) => output,
            super::check_exec::CommandRun::Cancelled => return Ok(()),
        };

    keep_capture(ctx.run_dir, output.stdout.as_bytes()).await?;
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
        })),
    )
    .await?;
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
