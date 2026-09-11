//! The repair cycle: an interpreted artifact that could not be read
//! gets a session of its own to rewrite it, instead of ending the node.
//!
//! **The repair is a session, not a re-run of the node.** That is what
//! makes one cycle serve every kind. The malformed file is on disk, so
//! the session that fixes it needs exactly two things: the shape the
//! document must have, and what is wrong with the one that is there. It
//! does not need the prompt that produced it, the author's `context:`,
//! or the node's own work — which is why this reaches a `kind: loop`
//! node whose ledger was written by a child task session the node no
//! longer holds, and why `kind: prompt` no longer pays for its whole
//! prompt a second time.
//!
//! A node enters the cycle when a declared artifact's *content* is
//! wrong, it resolves a runner, and `limits.max_artifact_repairs` has
//! something left. The three conditions are the whole rule; no kind is
//! named anywhere:
//!
//! - Only an artifact with a declared `kind:` is ever interpreted, so
//!   only such an artifact can fail on its content. A missing, empty or
//!   oversized file is an [`ArtifactFailure::File`], which no rewrite
//!   reaches.
//! - `runner:` and `artifacts:` are node-level keys, so any kind can
//!   declare both. A node that resolves no runner — neither its own
//!   `runner:` nor `defaults.runner` — has no agent to instruct, and
//!   that is a limit of the system rather than an omission: `kind: bash`
//!   is a command line and `kind: executor` is a JSON stdin, neither of
//!   which carries a channel an instruction rides on, and re-running
//!   either is a retry, not a repair.
//! - Each repair is a fresh attempt on the log — the failed lap's
//!   `node_failed` and then a `node_started` for the next — so a repair
//!   is visible in `status`, countable in `stats`, and replayable
//!   through a sequence the derivation already accepts.

use tokio_util::sync::CancellationToken;
use yunta_adapters::SessionRequest;
use yunta_core::diagnostic::{ArtifactFailure, Report};
use yunta_core::events::{EventPayload, NodeStartedPayload, TokenUsage};
use yunta_core::Node;

use crate::task_cycle::{dispatch_session, DispatchError, DispatchOutcome, SessionSetup};

use super::node_close::{fail_artifacts, fail_with_tokens, Close};
use super::node_exec::{cancelled_end, session_profile, NodeEnd};
use super::runner_resolve::{report_declarative_network, resolve_node_runner};
use super::step::Step;
use super::{RunCtx, RunError};

/// What follows an artifact failure.
pub(super) enum Next {
    /// A repair session ran to an answer. Whatever it answered, the
    /// files on disk are the verdict, so the caller verifies them again.
    /// The [`TokenUsage`] is what this attempt spent and only this one,
    /// so the terminal event that finally closes it counts it once.
    Wrote(TokenUsage),
    /// The node's end is already on the log: the artifacts failed with
    /// nothing left to try, or a repair session never reached an answer.
    Ended(NodeEnd),
}

/// How many more repair sessions this node can open.
///
/// `Exhausted` is both "no budget left" and "this node resolves no
/// runner to dispatch a session on"; either way no repair instruction is
/// produced, and the failure about to be recorded is the terminal one it
/// says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attempts {
    Remaining,
    Exhausted,
}

/// Records one artifact failure and, when another attempt is possible,
/// announces it and dispatches the repair session.
///
/// `spent` is how many repairs this close has already bought. Every
/// `node_failed` on the way is recorded here, including the one that
/// ends the cycle, so a caller only has to finish the node once
/// verification finally passes.
pub(super) async fn next(
    ctx: &RunCtx<'_>,
    node: &Node,
    close: &Close<'_>,
    spent: u32,
    tokens: TokenUsage,
    failures: Vec<ArtifactFailure>,
) -> Result<Next, RunError> {
    let budget = ctx.manifest.config.resolved_max_artifact_repairs();
    let repair = match attempts(ctx, node, spent, budget) {
        Attempts::Remaining => instruction(&failures),
        Attempts::Exhausted => None,
    };
    // `retryable` is the same answer as the instruction: a failure is
    // recorded as one to attempt again exactly when this cycle is about
    // to attempt it, so a terminal attempt never claims otherwise.
    fail_artifacts(ctx, node, failures, repair.is_some(), tokens).await?;
    let Some(repair) = repair else {
        return Ok(Next::Ended(NodeEnd::Failed));
    };
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeStarted(NodeStartedPayload {
            attempt: close.attempt + spent + 1,
        }),
    )
    .await?;
    session(ctx, node, close.cancel, &repair).await
}

/// Whether another repair session is possible: an agent to dispatch it
/// on, and budget to pay for it.
///
/// The runner is the node's own `runner:` or the config's
/// `defaults.runner` — the same fallback `resolve_node_runner` applies,
/// asked here without resolving anything, because a node with no runner
/// must fail on its artifacts rather than on a missing runner it was
/// never going to use.
fn attempts(ctx: &RunCtx<'_>, node: &Node, spent: u32, budget: u32) -> Attempts {
    let resolves_a_runner = node.runner.is_some()
        || ctx
            .manifest
            .config
            .defaults
            .as_ref()
            .is_some_and(|defaults| defaults.runner.is_some());
    if resolves_a_runner && spent < budget {
        Attempts::Remaining
    } else {
        Attempts::Exhausted
    }
}

/// One repair session: the shape every declared artifact must have, the
/// problems the last attempt left, and nothing else.
///
/// It carries no skills and no run tools. Skills are instruction for the
/// node's own work, and the per-run MCP endpoint is for coordinating
/// with siblings this session is not part of; a session whose whole job
/// is to write one file to a published shape needs neither, and
/// mounting them would bill the run for capabilities it cannot use.
async fn session(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &CancellationToken,
    repair: &str,
) -> Result<Next, RunError> {
    let prompt = match super::context_resolve::assemble_shapes(ctx, node).await? {
        // A node with no interpreted artifact cannot fail on content, so
        // the cycle never reaches here without shapes to mount; an empty
        // assembly still leaves the problems, which name their own file.
        Step::Value(Some(shapes)) => format!("{shapes}\n\n{repair}"),
        Step::Value(None) => repair.to_string(),
        Step::Ended(end) => return Ok(Next::Ended(end)),
    };
    let chosen = match resolve_node_runner(ctx, node).await? {
        Step::Value(chosen) => chosen,
        Step::Ended(end) => return Ok(Next::Ended(end)),
    };
    let adapter = &ctx.adapters[&chosen.adapter];
    report_declarative_network(ctx, node, adapter.as_ref(), &chosen.adapter).await?;

    let request = SessionRequest {
        prompt,
        cwd: ctx.worktree.to_path_buf(),
        model: Some(chosen.model),
        agent: chosen.agent,
        permissions: session_profile(node),
        env: SessionSetup::secrets_env(&ctx.manifest.config),
        edit_constraints: (!node.scope.is_empty()).then(|| node.scope.clone()),
        budget: ctx.session_budget().await?,
        adapter_settings: ctx.adapter_settings(&chosen.adapter),
        skills: Vec::new(),
        run_tools_endpoint: None,
    };
    let (outcome, tokens) = dispatch_session(
        adapter.as_ref(),
        request,
        cancel,
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
        // Never a resume: a repair starts from the file it is correcting,
        // not from the conversation that produced it.
        None,
    )
    .await
    .map_err(|error| match error {
        DispatchError::Adapter(source) => RunError::Spawn {
            node: node.id.clone(),
            source,
        },
        DispatchError::Audit(source) => RunError::Storage(source),
    })?;

    Ok(match outcome {
        DispatchOutcome::Completed { .. } => Next::Wrote(tokens),
        DispatchOutcome::Failed { message, retryable } => {
            Next::Ended(fail_with_tokens(ctx, node, message, retryable, tokens).await?)
        }
        // No terminal event means the engine synthesizes a retryable
        // failure — the adapter never invents one.
        DispatchOutcome::Crashed => Next::Ended(
            fail_with_tokens(
                ctx,
                node,
                "the repair session ended without a terminal event".to_string(),
                true,
                tokens,
            )
            .await?,
        ),
        DispatchOutcome::BudgetExceeded { reason } => {
            Next::Ended(fail_with_tokens(ctx, node, reason, false, tokens).await?)
        }
        DispatchOutcome::Cancelled => Next::Ended(cancelled_end(ctx, node).await?),
    })
}

/// What the writer of failed artifacts is told, so the next attempt
/// starts from the problems rather than from the prompt that already
/// produced the wrong file.
///
/// `None` unless every failure is one a rewrite fixes: a node that also
/// lost a file it never wrote has nothing to gain from being asked for
/// the rest again, and half-repairing would leave it failing on the same
/// missing file a session later.
///
/// The shape the document must have is deliberately absent here: the
/// repair session is given it as its own `artifact-shape` context, the
/// same block `context_resolve` publishes for every typed artifact, so
/// the session is billed for it once and has one copy to work from.
fn instruction(failures: &[ArtifactFailure]) -> Option<String> {
    if failures.is_empty() || !failures.iter().all(ArtifactFailure::is_repairable) {
        return None;
    }
    Some(
        failures
            .iter()
            .filter_map(ArtifactFailure::report)
            .map(rewrite)
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

/// One document's problems as an instruction to whoever wrote it: what
/// could not be read, and a numbered list of what to fix, each item
/// naming its subject the way the document names it.
fn rewrite(report: &Report) -> String {
    let mut text = format!(
        "The {} you wrote at {} could not be read. Fix these and write the file again:\n",
        report.document.label(),
        report.document.path
    );
    for (position, diagnostic) in report.diagnostics.iter().enumerate() {
        text.push_str(&format!("\n  {}. {diagnostic}", position + 1));
    }
    text
}

#[cfg(test)]
mod tests {
    use yunta_core::diagnostic::{
        Diagnostic, DocumentRef, FileProblem, Named, Problem, Report, RuleCode, Subject,
    };
    use yunta_core::shape::contract;
    use yunta_core::{ArtifactKind, TaskId};

    use super::*;

    fn ledger_report() -> Report {
        let task = Named::new(TaskId::from_static("t1"), 0);
        Report::new(
            DocumentRef::new(ArtifactKind::TaskLedger, "artifacts/plan.yaml"),
            vec![
                Diagnostic::new(
                    Subject::Criterion {
                        task: task.clone(),
                        index: 0,
                    },
                    Problem::wrong_shape(
                        yunta_core::diagnostic::ValueShape::String,
                        "a mapping with `cmd`",
                        "- cmd: \"cargo test\"",
                    ),
                ),
                Diagnostic::new(
                    Subject::Task(task),
                    Problem::rule(RuleCode::EmptyScope, "`scope` is empty"),
                ),
            ],
        )
    }

    #[test]
    fn the_instruction_names_every_problem_and_asks_for_the_file_again() {
        let text = instruction(&[ArtifactFailure::Content(ledger_report())])
            .expect("a document a rewrite fixes");
        assert!(text.contains("could not be read"), "{text}");
        assert!(text.contains("write the file again"), "{text}");
        assert!(text.contains("artifacts/plan.yaml"), "{text}");
        // Numbered, and each item names its subject the way the document
        // does — never the path a deserializer walked.
        assert!(text.contains("1. task `t1`, criterion 1: "), "{text}");
        assert!(text.contains("2. task `t1`: "), "{text}");
    }

    #[test]
    fn the_instruction_leaves_the_published_shape_to_the_context_block() {
        let text = instruction(&[ArtifactFailure::Content(ledger_report())])
            .expect("a document a rewrite fixes");
        // The repair session is given the shape as its own
        // `artifact-shape` context. Repeating it here would bill the
        // session for the same text twice and hand its reader two copies
        // to reconcile.
        assert!(
            !text.contains(&contract(ArtifactKind::TaskLedger)),
            "{text}"
        );
    }

    #[test]
    fn a_file_that_was_never_written_is_not_a_file_to_rewrite() {
        let node = yunta_core::NodeId::from_static("plan");
        // Asking for the rest again while one file has nothing to correct
        // would leave the node failing on that same file a session later.
        assert_eq!(
            instruction(&[
                ArtifactFailure::Content(ledger_report()),
                ArtifactFailure::file("artifacts/notes.md", FileProblem::Missing { node }),
            ]),
            None
        );
    }

    #[test]
    fn a_node_with_nothing_wrong_has_nothing_to_instruct() {
        assert_eq!(instruction(&[]), None);
    }
}
