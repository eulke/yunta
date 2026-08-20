//! The task cycle (T5.2, Contrato §5.2) — the part of the ledger cycle
//! that actually runs a task through pre-check, dispatch, post-check and
//! scope check. `yunta_engine::register` (T5.1) validates a ledger before
//! any of this; this module is what happens once a task is `ready`.
//!
//! The engine, never the agent, decides `done` (I5): [`run_task`] always
//! re-runs every criterion after dispatch, regardless of what the
//! session reported — an agent that claims success with red criteria
//! still leaves the task not-done.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use futures::StreamExt;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{
    Adapter, AgentEvent, AgentOutcome, Budget, PermissionProfile, SessionRequest,
};
use yunta_core::events::{CriterionType, TokenUsage};
use yunta_core::{Task, TaskId, YuntaError};

use crate::scope::{scope_check, ScopeCheckError, ScopeCheckResult};

#[derive(Debug, Error)]
pub enum TaskCycleError {
    #[error("failed to run criterion `{cmd}` for task `{task}`")]
    Criterion {
        task: TaskId,
        cmd: String,
        #[source]
        source: std::io::Error,
    },
    #[error("adapter failed to spawn a session for task `{task}`")]
    Spawn {
        task: TaskId,
        #[source]
        source: YuntaError,
    },
    #[error(transparent)]
    ScopeCheck(#[from] ScopeCheckError),
    #[error("failed to evaluate task `{task}`'s scope expansion request: {source}")]
    ScopeExpansion {
        task: TaskId,
        #[source]
        source: crate::scope_expansion::ScopeExpansionError,
    },
    #[error("failed to compute the working tree's hash for memoization: git {args} in `{cwd}`: {detail}")]
    TreeHash {
        args: String,
        cwd: std::path::PathBuf,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionRun {
    pub cmd: String,
    pub exit_code: i32,
    pub is_guard: bool,
    /// Whether this result came from §5.4's memoization cache instead of
    /// an actual execution — `criteria_checked` records it so recibo/replay
    /// show what ran versus what was reused, nothing verified in silence.
    pub reused: bool,
}

/// Per-run memoization cache (§5.4): a criterion's result is reused when
/// its command, the working tree's content, and the resolved config are
/// all unchanged since the last time it ran *in this run*. Never
/// cross-run — a fresh `Memo` per `execute_run` call is correct, not a
/// gap: a resumed run simply starts with a cold cache and re-verifies
/// once more than strictly necessary, which is safe (over-verifying),
/// unlike a stale cross-run cache (which would risk under-verifying).
///
/// The full key the Contrato names is `cmd + tree_hash + declared env +
/// resolved config` — `declared env` drops out here because criteria
/// have no `env:` field in this schema recorte (nothing to declare yet).
pub struct Memo {
    config_hash: String,
    cache: Mutex<HashMap<String, i32>>,
}

impl Memo {
    pub fn new(config_hash: impl Into<String>) -> Self {
        Self {
            config_hash: config_hash.into(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn key(&self, cmd: &str, tree_hash: &str) -> String {
        yunta_core::sha256_hex(format!("{cmd}\x00{tree_hash}\x00{}", self.config_hash).as_bytes())
    }

    fn get(&self, cmd: &str, tree_hash: &str) -> Option<i32> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.get(&self.key(cmd, tree_hash)).copied()
    }

    fn put(&self, cmd: &str, tree_hash: &str, exit_code: i32) {
        let key = self.key(cmd, tree_hash);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(key, exit_code);
    }
}

/// A fingerprint of `cwd`'s current content (§5.4): the commit it's on,
/// its full diff against that commit (tracked changes), and every
/// untracked file's own content hash — conservative on purpose. Missing
/// an untracked file's content from the fingerprint would let two
/// genuinely different trees hash the same and wrongly reuse a stale
/// result; a bare filename list (from `git status`) isn't enough since a
/// file can change content without its name changing.
async fn tree_hash(cwd: &Path) -> Result<String, TaskCycleError> {
    let git_error = |args: &str, detail: String| TaskCycleError::TreeHash {
        args: args.to_string(),
        cwd: cwd.to_path_buf(),
        detail,
    };
    let run_git = |args: &'static [&'static str]| async move {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .await
            .map_err(|e| git_error(&args.join(" "), e.to_string()))?;
        if !output.status.success() {
            return Err(git_error(
                &args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok::<_, TaskCycleError>(String::from_utf8_lossy(&output.stdout).into_owned())
    };

    let head = run_git(&["rev-parse", "HEAD"]).await?;
    let diff = run_git(&["diff", "HEAD"]).await?;
    let untracked = run_git(&["ls-files", "--others", "--exclude-standard"]).await?;

    let mut untracked_fingerprint = String::new();
    for path in untracked.lines() {
        let bytes = std::fs::read(cwd.join(path)).unwrap_or_default();
        untracked_fingerprint.push_str(path);
        untracked_fingerprint.push(':');
        untracked_fingerprint.push_str(&yunta_core::sha256_hex(&bytes));
        untracked_fingerprint.push('\n');
    }

    Ok(yunta_core::sha256_hex(
        format!("{head}\n{diff}\n{untracked_fingerprint}").as_bytes(),
    ))
}

/// The pre-check's verdict (§5.2 step 2): "esta fase valida al
/// validador" — a non-guard criterion that already passes, or a guard
/// that's already red, means the criteria themselves are wrong, not that
/// the (not-yet-started) work is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreCheckOutcome {
    Red,
    TrivialCriterion { cmd: String },
    BrokenGuard { cmd: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Completed {
        summary: String,
    },
    Failed {
        message: String,
        retryable: bool,
    },
    /// No terminal event at all (O2) — the engine synthesizes this, the
    /// adapter never emits it.
    Crashed,
    /// The engine cut the session via `interrupt` → `kill` (T3.3, O4):
    /// the token count from `Usage` events or the wall-clock timeout
    /// demanded it, independent of whether the adapter itself honored
    /// `SessionRequest.budget`.
    BudgetExceeded {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttemptRecord {
    pub attempt: u32,
    pub dispatch: DispatchOutcome,
    /// Tokens this attempt's session consumed, from its `Usage` events.
    pub tokens: TokenUsage,
    pub post_check: Vec<CriterionRun>,
    pub scope: ScopeCheckResult,
    pub succeeded: bool,
    /// §6.2/D73/T5.11: the agent's own expansion request this attempt, if
    /// it wrote one, and what the engine decided — `None` when no request
    /// file was found, the ordinary case. The caller (`loop_exec.rs`) owns
    /// emitting `scope_expansion_requested`/`granted`/`denied` and the
    /// D80 finding conversion from this; `run_task` only decides and
    /// widens `scope` for this attempt's own check when granted.
    pub scope_expansion: Option<crate::scope_expansion::ScopeExpansionOutcome>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskOutcome {
    Done,
    Blocked { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskCycleReport {
    pub task_id: TaskId,
    pub pre_check: Vec<CriterionRun>,
    pub attempts: Vec<AttemptRecord>,
    pub outcome: TaskOutcome,
    /// `true` when any attempt's scope-expansion request escalated (§6.2:
    /// `ask` mode, or `max_per_run` already exhausted) — neither is a
    /// verdict `run_task` can render alone, so the cycle stops retrying
    /// and the caller (`loop_exec.rs`, DI-01) puts the decision to
    /// `HumanInteraction` — pausing only when no live surface answers —
    /// rather than burning further sessions while one is owed.
    pub needs_human_decision: bool,
}

/// Default retry cap (§5.2: "cap configurable, default 2").
pub const DEFAULT_MAX_RETRIES: u32 = 2;

async fn run_criterion(task_id: &TaskId, cwd: &Path, cmd: &str) -> Result<i32, TaskCycleError> {
    let status = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .status()
        .await
        .map_err(|source| TaskCycleError::Criterion {
            task: task_id.clone(),
            cmd: cmd.to_string(),
            source,
        })?;
    Ok(status.code().unwrap_or(-1))
}

/// Tree hash computed once per call and shared across every criterion in
/// it (§5.4) — criteria are read-only, so the tree can't change between
/// them, and one `git` round-trip beats N.
async fn run_all_criteria(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    let tree_hash = tree_hash(cwd).await?;
    let mut runs = Vec::with_capacity(task.criteria.len());
    for criterion in &task.criteria {
        let (exit_code, reused) = match memo.get(&criterion.cmd, &tree_hash) {
            Some(exit_code) => (exit_code, true),
            None => {
                let exit_code = run_criterion(&task.id, cwd, &criterion.cmd).await?;
                memo.put(&criterion.cmd, &tree_hash, exit_code);
                (exit_code, false)
            }
        };
        runs.push(CriterionRun {
            cmd: criterion.cmd.clone(),
            exit_code,
            is_guard: criterion.r#type == Some(CriterionType::Guard),
            reused,
        });
    }
    Ok(runs)
}

/// Pre-check in rojo (§5.2 step 2): every non-`guard` criterion must
/// fail, every `guard` must pass. Runs every criterion regardless — the
/// report should show all of them, not stop at the first surprise.
pub async fn pre_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
) -> Result<(Vec<CriterionRun>, PreCheckOutcome), TaskCycleError> {
    let runs = run_all_criteria(task, cwd, memo).await?;

    let mut outcome = PreCheckOutcome::Red;
    for run in &runs {
        if matches!(outcome, PreCheckOutcome::Red) {
            if run.is_guard && run.exit_code != 0 {
                outcome = PreCheckOutcome::BrokenGuard {
                    cmd: run.cmd.clone(),
                };
            } else if !run.is_guard && run.exit_code == 0 {
                outcome = PreCheckOutcome::TrivialCriterion {
                    cmd: run.cmd.clone(),
                };
            }
        }
    }

    Ok((runs, outcome))
}

/// Post-check (§5.2 step 4): every criterion, guard or not, must now
/// pass.
pub async fn post_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    run_all_criteria(task, cwd, memo).await
}

/// Grace period between `interrupt` and the follow-up `kill` once a
/// budget has been exceeded — long enough for a session that closes
/// cleanly on interrupt to actually do so, short enough that a session
/// that ignores it doesn't stall the attempt.
const INTERRUPT_GRACE_PERIOD: Duration = Duration::from_millis(200);

/// Spawns one session from `request`, drains it to a terminal outcome
/// and reports the tokens it consumed. Shared by the task cycle and by
/// prompt-node execution (T4.1): the request differs, the enforcement
/// does not.
///
/// O4: the adapter passes `request.budget` along if its CLI supports it,
/// but enforcement is the engine's job either way — this counts `Usage`
/// and races the wall-clock deadline independent of that, and cuts the
/// session with `interrupt` → grace → `kill` (A4) when either budget is
/// exceeded.
pub(crate) async fn dispatch_session(
    adapter: &dyn Adapter,
    request: SessionRequest,
    cancel: &CancellationToken,
) -> Result<(DispatchOutcome, TokenUsage), YuntaError> {
    let budget = request.budget;
    let mut session = adapter.spawn(request).await?;

    // Carries the timeout `Duration` alongside its computed `Instant` so
    // the timeout-exceeded branch can report it without re-deriving it
    // from `budget.timeout`.
    let deadline = budget
        .timeout
        .map(|timeout| (tokio::time::Instant::now() + timeout, timeout));
    let mut tokens = TokenUsage::default();
    let mut terminal = None;
    let mut cancelled = false;

    {
        let mut stream = session.events();
        loop {
            // A node outside a `join: any` race passes a token nothing
            // ever cancels, so that branch simply never wins for it —
            // same `select!` shape either way.
            let next = if let Some((deadline_at, timeout)) = deadline {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        cancelled = true;
                        None
                    }
                    result = tokio::time::timeout_at(deadline_at, stream.next()) => {
                        match result {
                            Ok(next) => next,
                            Err(_) => {
                                terminal = Some(DispatchOutcome::BudgetExceeded {
                                    reason: format!("exceeded timeout of {timeout:?}"),
                                });
                                break;
                            }
                        }
                    }
                }
            } else {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        cancelled = true;
                        None
                    }
                    next = stream.next() => next,
                }
            };
            if cancelled {
                break;
            }

            let Some(event) = next else { break };

            match event {
                AgentEvent::Usage {
                    input_tokens,
                    output_tokens,
                    cached_input_tokens,
                } => {
                    tokens.input += input_tokens;
                    tokens.output += output_tokens;
                    if let Some(cached) = cached_input_tokens {
                        tokens.cached = Some(tokens.cached.unwrap_or(0) + cached);
                    }
                    let tokens_used = tokens.input + tokens.output;
                    if let Some(max_tokens) = budget.max_tokens {
                        if tokens_used > max_tokens {
                            terminal = Some(DispatchOutcome::BudgetExceeded {
                                reason: format!(
                                    "exceeded max_tokens {max_tokens} ({tokens_used} used)"
                                ),
                            });
                            break;
                        }
                    }
                }
                AgentEvent::Completed {
                    result: AgentOutcome { summary },
                } => {
                    terminal = Some(DispatchOutcome::Completed { summary });
                    break;
                }
                AgentEvent::Failed { error, retryable } => {
                    terminal = Some(DispatchOutcome::Failed {
                        message: error.message,
                        retryable,
                    });
                    break;
                }
                _ => {}
            }
        }
    } // the stream's borrow of `session` ends here — interrupt/kill need &mut self too.

    if cancelled || matches!(terminal, Some(DispatchOutcome::BudgetExceeded { .. })) {
        // A4: never leave anything running. Ordered termination first,
        // then forceful — mock has nothing to distinguish them, but a
        // real adapter's session may still close cleanly on interrupt.
        let _ = session.interrupt().await;
        tokio::time::sleep(INTERRUPT_GRACE_PERIOD).await;
        let _ = session.kill().await;
    }

    if cancelled {
        return Ok((
            DispatchOutcome::Failed {
                message: "interrupted: a sibling in this join: any group finished first"
                    .to_string(),
                retryable: false,
            },
            tokens,
        ));
    }

    Ok((terminal.unwrap_or(DispatchOutcome::Crashed), tokens))
}

/// Runs a task through the full cycle (§5.2): pre-check once, then
/// dispatch → post-check → scope-check per attempt, retrying with a
/// fresh session up to `max_retries` times before `Blocked`.
///
/// Never trusts the session's own outcome (I5): `succeeded` on each
/// attempt is decided entirely by re-running criteria and the scope
/// diff, regardless of whether the session reported `Completed`.
///
/// `permissions` is §6.1's runtime moment for criteria: every criterion
/// command is checked against the merged model before anything runs — a
/// violating criterion blocks the whole task citing the rule (a policy
/// outcome in the report, never an engine abort). The scan happens here,
/// at the cycle's single entry point, so the standalone
/// [`pre_check`]/[`post_check`] helpers stay pure building blocks.
/// `profile` is the node's own rung of the same ladder, forwarded to
/// every session this cycle opens.
/// `scope_expansion` carries the loop node's own §6.2 settings (absent
/// means the schema's own default, `deny`) plus how many expansions this
/// run has already granted before this task cycle started — the caller
/// derives that count from the log (§6.2's `max_per_run` is run-scoped,
/// not task-scoped), `run_task` only reads and threads it through.
/// `already_granted_paths` (DI-01) are the paths every *prior*
/// `scope_expansion_granted` on the log authorized for this task — a
/// human grant lands between attempts, so the retry's effective scope
/// must include them from the very first diff it evaluates.
#[allow(clippy::too_many_arguments)]
pub async fn run_task(
    task: &Task,
    instruction: &str,
    adapter: &dyn Adapter,
    cwd: &Path,
    max_retries: u32,
    budget: Budget,
    memo: &Memo,
    permissions: Option<&yunta_core::PermissionsConfig>,
    profile: PermissionProfile,
    scope_expansion: Option<&yunta_core::ScopeExpansion>,
    granted_so_far: u32,
    already_granted_paths: &[String],
) -> Result<TaskCycleReport, TaskCycleError> {
    for criterion in &task.criteria {
        if let Some(rule) = crate::permissions::command_violation(&criterion.cmd, permissions) {
            return Ok(TaskCycleReport {
                task_id: task.id.clone(),
                pre_check: Vec::new(),
                attempts: Vec::new(),
                outcome: TaskOutcome::Blocked { reason: rule },
                needs_human_decision: false,
            });
        }
    }

    let (pre_runs, pre_outcome) = pre_check(task, cwd, memo).await?;

    if !matches!(pre_outcome, PreCheckOutcome::Red) {
        let reason = match pre_outcome {
            PreCheckOutcome::TrivialCriterion { cmd } => format!(
                "criterion `{cmd}` already passes before any work — the criteria need fixing, not the task"
            ),
            PreCheckOutcome::BrokenGuard { cmd } => {
                format!("guard `{cmd}` is already red before any work started")
            }
            PreCheckOutcome::Red => unreachable!(),
        };
        return Ok(TaskCycleReport {
            task_id: task.id.clone(),
            pre_check: pre_runs,
            attempts: Vec::new(),
            outcome: TaskOutcome::Blocked { reason },
            needs_human_decision: false,
        });
    }

    let mut attempts = Vec::new();
    let mut granted_this_call = granted_so_far;
    for attempt in 1..=(max_retries + 1) {
        // §5.2 step 3: minimal brief — the node's instruction plus which
        // task is this session's, never the plan as prose. Every attempt
        // is a fresh session with the same request.
        let request = SessionRequest {
            prompt: format!(
                "{instruction}\n\nYour task: `{}` — {}. Stay within its declared scope.",
                task.id, task.title
            ),
            cwd: cwd.to_path_buf(),
            model: None,
            agent: None,
            permissions: profile,
            env: Default::default(),
            edit_constraints: Some(task.scope.clone()),
            budget,
            adapter_settings: Default::default(),
        };
        // A loop task's own cancellation (mid-execution, from outside)
        // isn't wired in this recorte — see T4.6's debt note in
        // docs/m0-status.md — so this token is never triggered.
        let (dispatch_outcome, tokens) =
            dispatch_session(adapter, request, &CancellationToken::new())
                .await
                .map_err(|source| TaskCycleError::Spawn {
                    task: task.id.clone(),
                    source,
                })?;

        // §6.2: the agent never widens its own scope — it may have left a
        // request behind, which this attempt's own worktree is the only
        // place to find (a fresh session per attempt, same cwd).
        let expansion_outcome =
            match crate::scope_expansion::load_request(cwd).map_err(|source| {
                TaskCycleError::ScopeExpansion {
                    task: task.id.clone(),
                    source,
                }
            })? {
                Some(expansion_request) => {
                    let mode = scope_expansion.map(|se| se.mode).unwrap_or_default();
                    let within = scope_expansion
                        .map(|se| se.within.as_slice())
                        .unwrap_or(&[]);
                    let max_per_run = scope_expansion.and_then(|se| se.max_per_run);
                    let (precheck_exit, decision) = crate::scope_expansion::evaluate(
                        mode,
                        within,
                        max_per_run,
                        granted_this_call,
                        &expansion_request,
                        cwd,
                    )
                    .await
                    .map_err(|source| TaskCycleError::ScopeExpansion {
                        task: task.id.clone(),
                        source,
                    })?;
                    if decision == crate::scope_expansion::Decision::Granted {
                        granted_this_call += 1;
                    }
                    Some(crate::scope_expansion::ScopeExpansionOutcome {
                        request: expansion_request,
                        precheck_exit,
                        decision,
                    })
                }
                None => None,
            };
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

        let post_runs = post_check(task, cwd, memo).await?;
        // §6.2: "el diff final se evalúa contra scope declarado más
        // ampliaciones autorizadas" — never against a denied or escalated
        // request's paths.
        let scope = scope_check(cwd, &effective_scope).await?;

        let criteria_green = post_runs.iter().all(|r| r.exit_code == 0);
        let succeeded = criteria_green && scope.violations.is_empty();
        let escalated = matches!(
            expansion_outcome.as_ref().map(|o| &o.decision),
            Some(crate::scope_expansion::Decision::Escalate)
        );

        attempts.push(AttemptRecord {
            attempt,
            dispatch: dispatch_outcome,
            tokens,
            post_check: post_runs,
            scope,
            succeeded,
            scope_expansion: expansion_outcome,
        });

        if succeeded {
            return Ok(TaskCycleReport {
                task_id: task.id.clone(),
                pre_check: pre_runs,
                attempts,
                outcome: TaskOutcome::Done,
                needs_human_decision: escalated,
            });
        }
        // A pending human decision means no further session should spend
        // budget while the run is about to pause for it.
        if escalated {
            return Ok(TaskCycleReport {
                task_id: task.id.clone(),
                pre_check: pre_runs,
                attempts,
                outcome: TaskOutcome::Blocked {
                    reason: "a scope expansion request needs a human decision".to_string(),
                },
                needs_human_decision: true,
            });
        }
    }

    Ok(TaskCycleReport {
        task_id: task.id.clone(),
        pre_check: pre_runs,
        attempts,
        needs_human_decision: false,
        outcome: TaskOutcome::Blocked {
            reason: format!(
                "criteria still red or scope violated after {} attempt(s)",
                max_retries + 1
            ),
        },
    })
}
