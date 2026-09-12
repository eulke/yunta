//! One attempt of a task: the session it opens, the scope check on what
//! it changed, and what it tells `run_task` to do next.

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, Budget, PermissionProfile, SessionRequest};
use yunta_core::events::{CapabilityDegradedPayload, EventPayload, TokenUsage};
use yunta_core::Capability;
use yunta_core::Task;

use super::criteria::{post_check, Memo};
use super::session::{dispatch_session, DispatchError, SessionObserver, SessionSetup};
use super::{AttemptRecord, DispatchOutcome, TaskCycleError, TaskOutcome};
use crate::process::Supervision;
use crate::scope::scope_check;

/// Everything one attempt of [`run_task`] reads: the per-cycle context that
/// never changes between attempts, so an attempt takes just this and its
/// number.
pub(super) struct AttemptParams<'a> {
    pub(super) task: &'a Task,
    pub(super) instruction: &'a str,
    pub(super) adapter: &'a dyn Adapter,
    pub(super) cwd: &'a Path,
    pub(super) budget: Budget,
    pub(super) memo: &'a Memo,
    pub(super) profile: PermissionProfile,
    pub(super) scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    pub(super) max_expansion_files: usize,
    pub(super) grants: &'a crate::scope_expansion::GrantLedger,
    pub(super) already_granted_paths: &'a [String],
    pub(super) audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    pub(super) cancel: &'a CancellationToken,
    pub(super) setup: &'a SessionSetup,
    pub(super) supervision: Supervision<'a>,
}

/// What one attempt tells [`run_task`] to do next.
pub(super) enum AttemptStep {
    /// The cycle is over — record this attempt and report this outcome.
    Stop {
        record: AttemptRecord,
        outcome: TaskOutcome,
        needs_human_decision: bool,
    },
    /// Not settled — record this attempt and dispatch another.
    Again(AttemptRecord),
}

/// One attempt of the task cycle: opens a fresh session (run tools an offer
/// that degrades, never a contract), dispatches it, then verifies the result
/// the engine's own way — the agent's `Completed` never counts, only re-run
/// criteria and a clean scope diff. Returns what the adapter staged this
/// attempt alongside the step [`run_task`] acts on.
pub(super) async fn run_one_attempt(
    params: &AttemptParams<'_>,
    attempt: u32,
) -> Result<(Vec<PathBuf>, AttemptStep), TaskCycleError> {
    let (last_staged, dispatch_outcome, tokens) = open_and_dispatch(params).await?;
    let &AttemptParams {
        task,
        cwd,
        memo,
        already_granted_paths,
        supervision,
        ..
    } = params;

    // A cancelled dispatch ends the cycle right here — no post-check, no
    // verdict, no retry. The attempt is on record; what the cancellation
    // means for the task is the caller's decision, because only it knows
    // which token fired.
    if matches!(dispatch_outcome, DispatchOutcome::Cancelled) {
        let record = AttemptRecord {
            attempt,
            dispatch: dispatch_outcome,
            tokens,
            post_check: Vec::new(),
            scope: crate::scope::ScopeCheckResult::default(),
            succeeded: false,
            scope_expansion: None,
        };
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Interrupted,
                needs_human_decision: false,
            },
        ));
    }

    let expansion_outcome = evaluate_scope_expansion(params).await?;
    let granted_paths: &[String] = expansion_outcome
        .as_ref()
        .filter(|outcome| outcome.decision == crate::scope_expansion::Decision::Granted)
        .map(|outcome| outcome.request.paths.as_slice())
        .unwrap_or(&[]);
    let effective_scope: Vec<String> = task
        .scope
        .iter()
        .cloned()
        .chain(already_granted_paths.iter().cloned())
        .chain(granted_paths.iter().cloned())
        .collect();

    let post_runs = post_check(task, cwd, memo, supervision).await?;
    // The final diff is evaluated against the declared scope plus any
    // authorized expansions — never against a denied or escalated request's
    // paths.
    let scope = scope_check(cwd, &effective_scope, &last_staged).await?;

    let criteria_green = post_runs.iter().all(|r| r.exit_code == 0);
    let succeeded = criteria_green && scope.violations.is_empty();
    let escalated = matches!(
        expansion_outcome.as_ref().map(|o| &o.decision),
        Some(crate::scope_expansion::Decision::Escalate)
    );
    // The session declared its failure won't yield to another try — captured
    // before the outcome moves into the record below.
    let non_retryable_failure = matches!(
        dispatch_outcome,
        DispatchOutcome::Failed {
            retryable: false,
            ..
        }
    );

    let record = AttemptRecord {
        attempt,
        dispatch: dispatch_outcome,
        tokens,
        post_check: post_runs,
        scope,
        succeeded,
        scope_expansion: expansion_outcome,
    };

    if succeeded {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Done,
                needs_human_decision: escalated,
            },
        ));
    }
    // A pending human decision means no further session should spend budget
    // while the run is about to pause for it.
    if escalated {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Blocked {
                    reason: "a scope expansion request needs a human decision".to_string(),
                },
                needs_human_decision: true,
            },
        ));
    }
    // A failure the session marked non-retryable ends the cycle now: the
    // criteria were still verified above (the engine never trusts the
    // session's own verdict), and having found them unmet, another attempt
    // would only spend budget on the same dead end.
    if non_retryable_failure {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Blocked {
                    reason: "the session reported a non-retryable failure and the criteria \
                             are still red"
                        .to_string(),
                },
                needs_human_decision: false,
            },
        ));
    }
    Ok((last_staged, AttemptStep::Again(record)))
}

/// Opens a fresh session for one attempt and drives it to a terminal
/// outcome: a per-attempt run-tools listener (its bind failure degrades to
/// no tools, recorded, never fatal), the task's brief, and the dispatch
/// itself. Returns what the adapter staged, the dispatch outcome, and the
/// tokens it spent.
async fn open_and_dispatch(
    params: &AttemptParams<'_>,
) -> Result<(Vec<PathBuf>, DispatchOutcome, TokenUsage), TaskCycleError> {
    let &AttemptParams {
        task,
        instruction,
        adapter,
        cwd,
        budget,
        profile,
        audit,
        cancel,
        setup,
        ..
    } = params;
    // A fresh listener + credential per attempt — held across the dispatch,
    // dead with it. A bind failure degrades (the session runs without run
    // tools) rather than sinking the attempt: the tools are an offer, the
    // task's own criteria are the contract.
    let run_tools = match &setup.run_tools {
        Some(access) => match crate::run_tools::open_session_listener(
            access.clone(),
            Some(task.id.clone()),
            cwd.to_path_buf(),
        )
        .await
        {
            Ok(session) => Some(session),
            Err(e) => {
                // Recorded, not warned: the attempt runs without run tools,
                // and the log says so and why.
                if let Some((observer, obs_node)) = audit {
                    observer
                        .emit_session_event(
                            obs_node,
                            EventPayload::CapabilityDegraded(CapabilityDegradedPayload {
                                capability: Capability::RunTools,
                                adapter: adapter.id().clone(),
                                policy_applied: format!("the attempt runs without run tools: {e}"),
                            }),
                        )
                        .await
                        .map_err(|source| TaskCycleError::Audit {
                            task: task.id.clone(),
                            source,
                        })?;
                }
                None
            }
        },
        None => None,
    };
    // Minimal brief — the node's instruction plus which task is this
    // session's, never the plan as prose. Every attempt is a fresh session
    // with the same request.
    let mut prompt = format!(
        "{instruction}\n\nYour task: `{}` — {}. Stay within its declared scope.",
        task.id, task.title
    );
    // The node's own declared artifacts are this session's to hand over:
    // the file a `loop` node closes on is written from what its task
    // sessions submit and report.
    if let Some(notice) = crate::run_tools::submission_notice(
        run_tools.as_ref(),
        setup
            .run_tools
            .as_ref()
            .map(|access| access.declared.as_slice())
            .unwrap_or_default(),
        None,
    ) {
        prompt.push_str(&notice);
    }
    let request = SessionRequest {
        prompt,
        cwd: cwd.to_path_buf(),
        model: None,
        agent: None,
        permissions: profile,
        env: setup.env.clone(),
        edit_constraints: Some(task.scope.clone()),
        budget,
        adapter_settings: setup.adapter_settings.clone(),
        skills: setup.skills.clone(),
        run_tools_endpoint: run_tools.as_ref().map(|session| session.endpoint.clone()),
        // A task session produces no declared artifact of its own:
        // the ledger it works from was written by the node that
        // declared it, and its work lands in the worktree.
        artifact_dir: None,
        scratch_dir: Some(
            crate::session_dir::SessionSlot::Task(&setup.node, &task.id)
                .scratch_dir(&setup.run_dir),
        ),
    };
    let last_staged = adapter.staged_paths(&request);
    let (dispatch_outcome, tokens) = dispatch_session(adapter, request, cancel, audit, None)
        .await
        .map_err(|error| match error {
            DispatchError::Adapter(source) => TaskCycleError::Spawn {
                task: task.id.clone(),
                source,
            },
            DispatchError::Audit(source) => TaskCycleError::Audit {
                task: task.id.clone(),
                source,
            },
        })?;
    Ok((last_staged, dispatch_outcome, tokens))
}

/// Reads the agent's own scope-expansion request from this attempt's
/// worktree (a fresh session per attempt leaves it there, not on the log)
/// and evaluates it against the node's declared mode, `within` set and cap.
/// `None` when the attempt left no request — the ordinary case.
async fn evaluate_scope_expansion(
    params: &AttemptParams<'_>,
) -> Result<Option<crate::scope_expansion::ScopeExpansionOutcome>, TaskCycleError> {
    let &AttemptParams {
        task,
        cwd,
        scope_expansion,
        max_expansion_files,
        grants,
        supervision,
        ..
    } = params;
    let Some(expansion_request) = crate::scope_expansion::load_request(cwd).map_err(|source| {
        TaskCycleError::ScopeExpansion {
            task: task.id.clone(),
            source,
        }
    })?
    else {
        return Ok(None);
    };
    let mode = scope_expansion.map(|se| se.mode).unwrap_or_default();
    let within = scope_expansion
        .map(|se| se.within.as_slice())
        .unwrap_or(&[]);
    let max_per_run = scope_expansion.and_then(|se| se.max_per_run);
    let (precheck_exit, decision) = crate::scope_expansion::evaluate(
        mode,
        within,
        max_per_run,
        max_expansion_files,
        grants,
        &expansion_request,
        cwd,
        supervision,
    )
    .await
    .map_err(|source| TaskCycleError::ScopeExpansion {
        task: task.id.clone(),
        source,
    })?;
    Ok(Some(crate::scope_expansion::ScopeExpansionOutcome {
        request: expansion_request,
        precheck_exit,
        decision,
    }))
}
