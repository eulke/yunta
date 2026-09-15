//! `kind: check` execution. A check builtin's shape — run a command,
//! parse or compare, never touch an agent — is distinct enough from
//! bash/prompt/parallel dispatch to stand on its own, and the one way
//! the engine runs a verification command of its own lives here, for the
//! checks and for the baseline a run captures when it is created.

use std::path::Path;

use yunta_core::events::{
    BaselineCapturedPayload, EventPayload, FindingSeverity, StoredEvent, TokenUsage,
};
use yunta_core::{CheckBuiltin, Node};

use super::node_close::{close_node, fail, Close};
use super::node_exec::NodeEnd;
use super::{RunCtx, RunError};
use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome, Supervision};
use yunta_core::events::NodeEvent;

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
}

/// A verification command's run: done with its output, or cut short —
/// the caller turns the latter into the fate of whatever asked for it.
pub(super) enum CommandRun {
    Done(CommandOutput),
    Cancelled,
}

/// Runs `cmd` in `cwd` to completion and captures its stdout — unlike a
/// bash *node*, a command the engine runs to verify something is not
/// agent-visible work, so its stdout is data to parse, not a stream to
/// relay. The command runs in its own process group and dies with its
/// whole tree when `supervision` is cancelled.
pub(super) async fn run_command(
    supervision: Supervision<'_>,
    cwd: &Path,
    cmd: &str,
) -> Result<CommandRun, RunError> {
    let command = GovernedCommand::shell(cwd, cmd).stderr(Capture::Discard);
    match spawn_governed(command, supervision).await? {
        Outcome::Exited { status, stdout, .. } => Ok(CommandRun::Done(CommandOutput {
            exit_code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
        })),
        // A verification command has no timeout of its own: stopping
        // early means the run was cancelled.
        Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => Ok(CommandRun::Cancelled),
    }
}

/// `baseline_compare`: re-runs the suite the run captured when it was
/// created and fails if something that passed then stops passing. Every
/// such node compares — the capture is the run's, taken before any node
/// of it ran, so the first comparison covers the work before it like any
/// later one.
///
/// The suite is the captured one, read off the log rather than resolved
/// from config again: what the comparison is against is what actually
/// ran.
async fn execute_baseline_compare(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<NodeEnd, RunError> {
    let Some(captured) = captured_baseline(&ctx.load_events().await?) else {
        return fail(
            ctx,
            node,
            "check `baseline_compare` has nothing to compare against: a run captures its \
             baseline when it is created, and this one captured none — declare \
             `baseline.suite` in config"
                .to_string(),
            false,
        )
        .await;
    };

    let output = match run_command(ctx.supervision(cancel), ctx.worktree, &captured.command).await?
    {
        CommandRun::Done(output) => output,
        CommandRun::Cancelled => return super::node_exec::cancelled_end(ctx, node).await,
    };

    if captured.results.exit_code == 0 && output.exit_code != 0 {
        fail(
            ctx,
            node,
            format!(
                "regression: `{}` passed at baseline (exit 0) but now exits {}",
                captured.command, output.exit_code
            ),
            false,
        )
        .await
    } else {
        close_node(
            ctx,
            node,
            Close::new(
                format!("no regression vs baseline (exit {})", output.exit_code),
                TokenUsage::default(),
            ),
        )
        .await
    }
}

/// What the run captured when it was created — `None` for a run whose
/// config named no suite to capture, which is the only way a log carries
/// none.
fn captured_baseline(events: &[StoredEvent]) -> Option<BaselineCapturedPayload> {
    events.iter().find_map(|event| match event.payload() {
        Some(EventPayload::Node(NodeEvent::BaselineCaptured(payload))) => Some(payload.clone()),
        _ => None,
    })
}

/// `coverage_gate`: `coverage.cmd`'s stdout must contain a bare
/// `NN[.NN]%` — the last one found is taken as the measured coverage, per
/// `CoverageConfig`'s own documented convention, since no parsing
/// contract is fixed elsewhere.
async fn execute_coverage_gate(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<NodeEnd, RunError> {
    let Some(coverage) = &ctx.manifest.config.coverage else {
        return fail(
            ctx,
            node,
            "check `coverage_gate` needs `coverage.cmd` and `coverage.threshold` configured"
                .to_string(),
            false,
        )
        .await;
    };

    let output = match run_command(ctx.supervision(cancel), ctx.worktree, &coverage.cmd).await? {
        CommandRun::Done(output) => output,
        CommandRun::Cancelled => return super::node_exec::cancelled_end(ctx, node).await,
    };
    let Some(measured) = parse_last_percentage(&output.stdout) else {
        return fail(
            ctx,
            node,
            format!(
                "`{}` produced no `NN[.NN]%` coverage figure in its stdout",
                coverage.cmd
            ),
            false,
        )
        .await;
    };

    if measured < coverage.threshold {
        fail(
            ctx,
            node,
            format!(
                "coverage {measured}% is below the {}% threshold",
                coverage.threshold
            ),
            false,
        )
        .await
    } else {
        close_node(
            ctx,
            node,
            Close::new(
                format!(
                    "coverage {measured}% meets the {}% threshold",
                    coverage.threshold
                ),
                TokenUsage::default(),
            ),
        )
        .await
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
/// at or above `max_severity`. Ranked by declaration order
/// (`Blocking` worst, `Note` least) since `FindingSeverity` has no `Ord`
/// of its own to reuse.
async fn execute_findings_gate(
    ctx: &RunCtx<'_>,
    node: &Node,
    max_severity: FindingSeverity,
) -> Result<NodeEnd, RunError> {
    let state = ctx.run_view().await?.state;
    let effective = state.effective_findings();
    let offending: Vec<&str> = effective
        .iter()
        .filter(|finding| severity_rank(finding.severity) <= severity_rank(max_severity))
        .map(|finding| finding.id.as_str())
        .collect();

    if offending.is_empty() {
        close_node(
            ctx,
            node,
            Close::new(
                format!("no finding at or above {max_severity:?}"),
                TokenUsage::default(),
            ),
        )
        .await
    } else {
        fail(
            ctx,
            node,
            format!(
                "{} finding(s) at or above {max_severity:?}: {}",
                offending.len(),
                offending.join(", ")
            ),
            false,
        )
        .await
    }
}

fn severity_rank(severity: FindingSeverity) -> u8 {
    match severity {
        FindingSeverity::Blocking => 0,
        FindingSeverity::Major => 1,
        FindingSeverity::Minor => 2,
        FindingSeverity::Note => 3,
    }
}
