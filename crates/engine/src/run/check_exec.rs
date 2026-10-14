//! `kind: check` execution. A check builtin's shape — run a command,
//! parse or compare, never touch an agent — is distinct enough from
//! bash/prompt/parallel dispatch to stand on its own, and the one way
//! the engine runs a verification command of its own lives here, for the
//! checks and for the baseline a run's lineage measures.

use std::path::Path;

use yunta_core::events::Failure;
use yunta_core::events::{EngineFact, FindingSeverity, NodeOutcome, RedCheck, TokenUsage};
use yunta_core::{CheckBuiltin, ConfigKey, Node};

use super::node_close::{close_node, fail, fail_with, Close};
use super::node_exec::NodeEnd;
use super::{RunCtx, RunError};
use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome, Supervision};

/// `kind: check`: the engine evaluates its own data,
/// never a person — no session, no tokens spent. Each builtin's config
/// (`baseline:`/`coverage:`) is workflow-config, not node-level, so a node
/// missing it fails with a diagnostic naming what to declare rather than
/// panicking or silently skipping.
pub(super) async fn execute_check(
    ctx: &RunCtx<'_>,
    node: &Node,
    builtin: &CheckBuiltin,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<NodeEnd, RunError> {
    match builtin {
        CheckBuiltin::BaselineCompare => execute_baseline_compare(ctx, node, cancel).await,
        CheckBuiltin::CoverageGate => execute_coverage_gate(ctx, node, cancel).await,
        // `findings_gate` reads the log and runs no command of its own,
        // so nothing of it is cancellable.
        CheckBuiltin::FindingsGate { max_severity } => {
            execute_findings_gate(ctx, node, *max_severity).await
        }
    }
}

pub(super) struct CommandOutput {
    pub(super) exit_code: i32,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

/// A verification command's run: done with its output, or cut short —
/// the caller turns the latter into the fate of whatever asked for it.
pub(super) enum CommandRun {
    Done(CommandOutput),
    Cancelled,
}

/// Runs `cmd` in `cwd` to completion and captures its stdout and stderr —
/// unlike a bash *node*, a command the engine runs to verify something is
/// not agent-visible work, so its output is data to parse, not a stream to
/// relay. The command runs in its own process group and dies with its
/// whole tree when `supervision` is cancelled.
pub(super) async fn run_command(
    supervision: Supervision<'_>,
    cwd: &Path,
    cmd: &str,
) -> Result<CommandRun, RunError> {
    let command = GovernedCommand::shell(cwd, cmd).stderr(Capture::Collect);
    match spawn_governed(command, supervision).await? {
        Outcome::Exited {
            status,
            stdout,
            stderr,
            ..
        } => Ok(CommandRun::Done(CommandOutput {
            exit_code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })),
        // A verification command has no timeout of its own: stopping
        // early means the run was cancelled.
        Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => Ok(CommandRun::Cancelled),
    }
}

/// `baseline_compare`: re-runs the suite this run's lineage measured and
/// fails if something that passed then stops passing. Every such node
/// compares against that one measurement — taken before any node of the
/// lineage ran — so the first comparison covers the work before it like
/// any later one, a parent's included.
///
/// The suite is the measured one, read off the run's own fold rather
/// than resolved from config again: what the comparison is against is
/// what actually ran.
async fn execute_baseline_compare(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<NodeEnd, RunError> {
    let held = ctx.run_view().await?.state.run.baseline().cloned();
    // The lineage measures before its first node, and only when its
    // config names a suite: a run that holds no measurement was born under
    // one that names none.
    let Some(captured) = held else {
        return unset(ctx, node, ConfigKey::BaselineSuite).await;
    };
    // A suite that was already red when the lineage measured it cannot
    // show that anything stopped passing, so running it again would
    // spend its whole duration on an answer known before it starts.
    if !captured.passed() {
        let outcome = NodeOutcome::Fact(EngineFact::NothingToCompare {
            command: captured.command.clone(),
            exit_code: captured.results.exit_code,
        });
        return close_node(ctx, node, Close::new(outcome, TokenUsage::default())).await;
    }

    // The same memo the criteria use: two comparisons of one suite on a
    // tree nothing changed in between run it once, and the second says
    // so rather than paying for an answer this invocation already has.
    let ran = ctx
        .memo
        .exit_code(&captured.command, ctx.worktree, ctx.supervision(cancel))
        .await?;
    if cancel.is_cancelled() {
        return super::node_exec::cancelled_end(ctx, node).await;
    }

    if ran.exit_code != 0 {
        fail_with(ctx, node, regressed(&captured, &ran), TokenUsage::default()).await
    } else {
        // The memo does not say who asked first, so the fact says only
        // whether this invocation already had the answer.
        let outcome = NodeOutcome::Fact(EngineFact::NoRegression {
            exit_code: ran.exit_code,
            reused: ran.reused,
        });
        close_node(ctx, node, Close::new(outcome, TokenUsage::default())).await
    }
}

/// The red check a comparison that found the suite failing fails with: the
/// command that passed when it was measured, the exit code it fails with
/// now, and what it printed last, which is what says why.
fn regressed(
    captured: &yunta_core::events::BaselineCapturedPayload,
    ran: &crate::task_cycle::Memoized,
) -> Failure {
    let red = RedCheck::Regression {
        cmd: captured.command.clone(),
        exit_code: ran.exit_code,
        tail: ran
            .output
            .as_ref()
            .map(|output| output.tail())
            .unwrap_or_default(),
    };
    Failure::Red { red }
}

/// `coverage_gate`: `coverage.cmd` must exit 0, and its stdout must contain
/// a bare `NN[.NN]%` — the last one found is taken as the measured coverage.
/// A command that exits otherwise has measured nothing, so its exit is judged
/// before any figure is read.
async fn execute_coverage_gate(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<NodeEnd, RunError> {
    let Some(coverage) = &ctx.manifest.config.coverage else {
        return unset(ctx, node, ConfigKey::Coverage).await;
    };

    let output = match run_command(ctx.supervision(cancel), ctx.worktree, &coverage.cmd).await? {
        CommandRun::Done(output) => output,
        CommandRun::Cancelled => return super::node_exec::cancelled_end(ctx, node).await,
    };
    if output.exit_code != 0 {
        let conclusion =
            crate::process::output::conclusion(output.stdout.as_bytes(), output.stderr.as_bytes());
        let red = RedCheck::Command {
            cmd: coverage.cmd.clone(),
            exit_code: output.exit_code,
            conclusion,
            output: None,
        };
        return fail_with(ctx, node, Failure::Red { red }, TokenUsage::default()).await;
    }
    let Some(measured) = parse_last_percentage(&output.stdout) else {
        let red = RedCheck::Coverage {
            cmd: coverage.cmd.clone(),
            measured: None,
            threshold: coverage.threshold,
        };
        return fail_with(ctx, node, Failure::Red { red }, TokenUsage::default()).await;
    };

    if measured < coverage.threshold {
        let red = RedCheck::Coverage {
            cmd: coverage.cmd.clone(),
            measured: Some(measured),
            threshold: coverage.threshold,
        };
        fail_with(ctx, node, Failure::Red { red }, TokenUsage::default()).await
    } else {
        let outcome = NodeOutcome::Fact(EngineFact::CoverageMet {
            measured,
            threshold: coverage.threshold,
        });
        close_node(ctx, node, Close::new(outcome, TokenUsage::default())).await
    }
}

/// Finds the last `NN[.NN]%` substring in `text` and parses its number —
/// hand-rolled rather than pulling in a regex dependency for one bounded
/// scan.
fn parse_last_percentage(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let mut best = None;
    for i in 0..bytes.len() {
        if bytes.get(i) != Some(&b'%') {
            continue;
        }
        let mut start = i;
        let mut seen_dot = false;
        while start > 0 {
            match bytes.get(start - 1) {
                Some(b'0'..=b'9') => start -= 1,
                Some(b'.') if !seen_dot => {
                    seen_dot = true;
                    start -= 1;
                }
                _ => break,
            }
        }
        if start < i {
            if let Ok(value) = text[start..i].parse::<f64>() {
                best = Some(value);
            }
        }
    }
    best
}

/// `findings_gate`: fails if any finding posted so far in this
/// run — raw, not deduped (`dedup_findings` is a reporting view;
/// a gate checking "does anything this severe exist" should not risk
/// under-counting two distinct findings a dedup heuristic conflated) — is
/// at or above `max_severity`, the ladder's own order (`blocking` worst,
/// `note` least).
async fn execute_findings_gate(
    ctx: &RunCtx<'_>,
    node: &Node,
    max_severity: FindingSeverity,
) -> Result<NodeEnd, RunError> {
    let state = ctx.run_view().await?.state;
    // A finding settled by its proof or by a person stands, and is past
    // what a gate on severity holds the run to.
    let effective = state.unsettled_findings();
    let offending: Vec<_> = effective
        .iter()
        .filter(|finding| finding.severity <= max_severity)
        .map(|finding| finding.id.clone())
        .collect();

    if offending.is_empty() {
        let outcome = NodeOutcome::Fact(EngineFact::NoFinding {
            severity: max_severity,
        });
        close_node(ctx, node, Close::new(outcome, TokenUsage::default())).await
    } else {
        let failure = Failure::FindingsOpen {
            at_or_above: max_severity,
            open_findings: offending,
        };
        fail(ctx, node, failure).await
    }
}

/// Fails `node` on a config key the run's frozen config leaves unset —
/// the failure no attempt of this run can change, and which says so.
pub(super) async fn unset(
    ctx: &RunCtx<'_>,
    node: &Node,
    key: ConfigKey,
) -> Result<NodeEnd, RunError> {
    fail_with(ctx, node, Failure::unset(key), TokenUsage::default()).await
}
