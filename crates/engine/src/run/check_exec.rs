//! `kind: check` execution. A check builtin's shape — run a command,
//! parse or compare, never touch an agent — is distinct enough from
//! bash/prompt/parallel dispatch to stand on its own, and the one way
//! the engine runs a verification command of its own lives here, for the
//! checks and for the baseline a run's lineage measures.

use std::path::Path;

use yunta_core::events::Failure;
use yunta_core::events::{FindingSeverity, TokenUsage};
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
    if let Some(end) = refuse_unchanged(ctx, node, builtin).await? {
        return Ok(end);
    }
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

/// Refuses an attempt a person asked for that would run a check on the
/// very tree its failed attempt ran on: a check that judges the tree
/// answers the same on the same tree, so the attempt fails at once,
/// naming the one that ran and what it failed with. The menu that
/// follows still offers another attempt — the person may change the
/// tree while it is open.
async fn refuse_unchanged(
    ctx: &RunCtx<'_>,
    node: &Node,
    builtin: &CheckBuiltin,
) -> Result<Option<NodeEnd>, RunError> {
    if !builtin.judges_the_tree() {
        return Ok(None);
    }
    let state = ctx.run_view().await?.state;
    let retried = state
        .choice_after_failure(&node.id)
        .is_some_and(|(_, choice)| {
            crate::reserved::ReservedOption::of(&choice.option)
                == Some(crate::reserved::ReservedOption::Retry)
        });
    let Some(failed) = state
        .nodes
        .get(&node.id)
        .and_then(|record| record.repeats())
        .filter(|_| retried)
    else {
        return Ok(None);
    };
    let failure = Failure::unchanged(failed.attempt, failed.failure.clone());
    fail_with(ctx, node, failure, false, TokenUsage::default())
        .await
        .map(Some)
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
        return close_node(
            ctx,
            node,
            Close::new(nothing_to_compare(&captured), TokenUsage::default()),
        )
        .await;
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
        fail(ctx, node, regression(&captured, &ran), false).await
    } else {
        close_node(
            ctx,
            node,
            Close::new(no_regression(&ran), TokenUsage::default()),
        )
        .await
    }
}

/// What a comparison that found the suite failing fails with: the exit
/// code it passed with and the one it fails with now, then what the
/// suite printed last, which is what says why.
fn regression(
    captured: &yunta_core::events::BaselineCapturedPayload,
    ran: &crate::task_cycle::Memoized,
) -> String {
    let said = ran
        .output
        .as_ref()
        .map(|output| output.tail().join("\n"))
        .unwrap_or_default();
    yunta_core::text::detailed(
        format!(
            "regression: `{}` passed at baseline (exit 0) but now exits {}",
            captured.command, ran.exit_code
        ),
        &said,
    )
}

/// What a comparison closes with when the measurement it would compare
/// against was already red: there was nothing that passed, so nothing
/// could be seen to stop passing.
fn nothing_to_compare(captured: &yunta_core::events::BaselineCapturedPayload) -> String {
    format!(
        "nothing to compare: `{}` was already red when the lineage measured it (exit {})",
        captured.command, captured.results.exit_code
    )
}

/// What a comparison that found no regression closes with: the suite's
/// exit code, and where the answer came from when this invocation
/// already had it.
fn no_regression(ran: &crate::task_cycle::Memoized) -> String {
    match ran.reused {
        false => format!("no regression vs baseline (exit {})", ran.exit_code),
        true => format!(
            "no regression vs baseline (exit {}, reused: same tree since an earlier compare)",
            ran.exit_code
        ),
    }
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
        return unset(ctx, node, ConfigKey::Coverage).await;
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

/// Fails `node` on a config key the run's frozen config leaves unset —
/// the failure no attempt of this run can change, and which says so.
pub(super) async fn unset(
    ctx: &RunCtx<'_>,
    node: &Node,
    key: ConfigKey,
) -> Result<NodeEnd, RunError> {
    fail_with(ctx, node, Failure::unset(key), false, TokenUsage::default()).await
}
