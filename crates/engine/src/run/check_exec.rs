//! `kind: check` execution — split out of
//! `node_exec.rs` once that file passed CLAUDE.md's ~500-line soft
//! ceiling; kept as its own module since a check builtin's shape (run a
//! command, parse or compare, never touch an agent) is distinct enough
//! from bash/prompt/parallel dispatch to stand alone.

use std::path::Path;

use yunta_core::events::{
    BaselineCapturedPayload, BaselineResults, EventPayload, FindingSeverity, TokenUsage,
};
use yunta_core::{CheckBuiltin, Node};

use crate::replay::derive;

use super::node_exec::{close_node, fail, NodeEnd};
use super::{RunCtx, RunError};

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
        // `findings_gate` reads artifacts, spawns nothing — there is no
        // wait point for a token to interrupt.
        CheckBuiltin::FindingsGate { max_severity } => {
            execute_findings_gate(ctx, node, *max_severity).await
        }
    }
}

struct CommandOutput {
    exit_code: i32,
    stdout: String,
}

/// A check command's run: done with its output, or cut by cancellation —
/// the caller turns the latter into the node's own fate.
enum CommandRun {
    Done(CommandOutput),
    Cancelled,
}

/// Runs `cmd` to completion and captures its stdout — unlike a bash
/// *node*, a check builtin's command is the engine's own verification
/// step, not agent-visible work, so its stdout is data to parse, not a
/// stream to relay. Cancel-aware: the command runs in its own
/// process group, registered in `engine.json`, and a fired token kills
/// the whole tree.
async fn run_command(
    ctx: &RunCtx<'_>,
    cwd: &Path,
    cmd: &str,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<CommandRun, RunError> {
    let mut std_cmd = std::process::Command::new("sh");
    std_cmd
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        std_cmd.process_group(0);
    }
    let mut child = tokio::process::Command::from(std_cmd)
        .spawn()
        .map_err(|source| RunError::Io {
            context: format!("run check command `{cmd}`"),
            source,
        })?;
    let _pgid_registration = crate::process_registry::register(
        ctx.process_registry.as_ref(),
        crate::process_registry::child_pid(&child),
    );

    let stdout_task = child.stdout.take().map(|mut pipe| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf).await;
            buf
        })
    });

    tokio::select! {
        _ = cancel.cancelled() => {
            if let Some(pid) = crate::process_registry::child_pid(&child) {
                super::node_exec::kill_process_group(pid).await;
            }
            let _ = child.wait().await;
            if let Some(task) = stdout_task {
                let _ = task.await;
            }
            Ok(CommandRun::Cancelled)
        }
        status = child.wait() => {
            let status = status.map_err(|source| RunError::Io {
                context: format!("run check command `{cmd}`"),
                source,
            })?;
            let stdout_bytes = match stdout_task {
                Some(task) => task.await.unwrap_or_default(),
                None => Vec::new(),
            };
            Ok(CommandRun::Done(CommandOutput {
                exit_code: status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            }))
        }
    }
}

/// `baseline_compare`: capture is lazy, on this builtin's own first
/// invocation in the run, instead of unconditionally at worktree creation
/// as the Contrato's prose describes — capturing eagerly would make
/// `create_run` async across its four call sites for a builtin most
/// workflows never use. Documented deviation, not a silent gap: the first
/// `baseline_compare` node always passes (it has nothing yet to compare
/// against) and every later one compares against that first run's result.
async fn execute_baseline_compare(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<NodeEnd, RunError> {
    let Some(baseline) = &ctx.manifest.config.baseline else {
        return fail(
            ctx,
            node,
            "check `baseline_compare` needs `baseline.suite` configured".to_string(),
            false,
        );
    };

    let already_captured = ctx
        .load_events()?
        .into_iter()
        .find_map(|event| match event.payload {
            EventPayload::BaselineCaptured(payload) => Some(payload),
            _ => None,
        });

    let output = match run_command(ctx, ctx.worktree, &baseline.suite, cancel).await? {
        CommandRun::Done(output) => output,
        CommandRun::Cancelled => return super::node_exec::cancelled_end(ctx, node),
    };

    match already_captured {
        None => {
            ctx.emit(
                Some(&node.id),
                EventPayload::BaselineCaptured(BaselineCapturedPayload {
                    command: baseline.suite.clone(),
                    results: BaselineResults {
                        exit_code: output.exit_code,
                        summary: output
                            .stdout
                            .lines()
                            .rev()
                            .take(5)
                            .collect::<Vec<_>>()
                            .join("\n"),
                    },
                    hash: yunta_core::sha256_hex(output.stdout.as_bytes()),
                }),
            )?;
            close_node(
                ctx,
                node,
                format!("baseline captured (exit {})", output.exit_code),
                TokenUsage::default(),
            )
            .await
        }
        Some(captured) => {
            if captured.results.exit_code == 0 && output.exit_code != 0 {
                fail(
                    ctx,
                    node,
                    format!(
                        "regression: `{}` passed at baseline (exit 0) but now exits {}",
                        baseline.suite, output.exit_code
                    ),
                    false,
                )
            } else {
                close_node(
                    ctx,
                    node,
                    format!("no regression vs baseline (exit {})", output.exit_code),
                    TokenUsage::default(),
                )
                .await
            }
        }
    }
}

/// `coverage_gate`: `coverage.cmd`'s stdout must contain a bare
/// `NN[.NN]%` — the last one found is taken as the measured coverage, per
/// `CoverageConfig`'s own documented convention (the Contrato's prose
/// doesn't specify a parsing contract).
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
        );
    };

    let output = match run_command(ctx, ctx.worktree, &coverage.cmd, cancel).await? {
        CommandRun::Done(output) => output,
        CommandRun::Cancelled => return super::node_exec::cancelled_end(ctx, node),
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
        );
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
    } else {
        close_node(
            ctx,
            node,
            format!(
                "coverage {measured}% meets the {}% threshold",
                coverage.threshold
            ),
            TokenUsage::default(),
        )
        .await
    }
}

/// Finds the last `NN[.NN]%` substring in `text` and parses its number —
/// hand-rolled rather than pulling in a regex dependency for one bounded
/// scan (CLAUDE.md: "¿alcanza std o algo ya presente?").
fn parse_last_percentage(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let mut best = None;
    for i in 0..bytes.len() {
        if bytes[i] != b'%' {
            continue;
        }
        let mut start = i;
        let mut seen_dot = false;
        while start > 0 {
            match bytes[start - 1] {
                b'0'..=b'9' => start -= 1,
                b'.' if !seen_dot => {
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
    let state = derive(&ctx.load_events()?);
    let offending: Vec<&str> = state
        .findings
        .iter()
        .filter(|finding| severity_rank(finding.severity) <= severity_rank(max_severity))
        .map(|finding| finding.id.as_str())
        .collect();

    if offending.is_empty() {
        close_node(
            ctx,
            node,
            format!("no finding at or above {max_severity:?}"),
            TokenUsage::default(),
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
