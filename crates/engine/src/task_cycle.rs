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
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use futures::StreamExt;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{
    Adapter, AgentEvent, AgentOutcome, Budget, PermissionProfile, SessionRequest,
};
use yunta_core::events::{Criterion, CriterionType, EventPayload, TokenUsage};
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
    /// Wall-clock milliseconds the execution took (DI-15) — what D62's
    /// learned ordering feeds on. `None` when `reused` (nothing ran).
    pub duration_ms: Option<u64>,
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
    /// DI-15: observed wall-clock durations per criterion command, this
    /// invocation only — the same lifetime discipline as the result
    /// cache above (a resume starts cold and re-learns, which only
    /// costs one declared-order pass). Keyed by the bare command, not
    /// the memo key: a criterion's cost profile survives tree changes,
    /// which is exactly when the ordering matters (a memo hit never
    /// re-runs anything, so there is nothing to reorder).
    durations: Mutex<HashMap<String, Vec<u64>>>,
}

impl Memo {
    pub fn new(config_hash: impl Into<String>) -> Self {
        Self {
            config_hash: config_hash.into(),
            cache: Mutex::new(HashMap::new()),
            durations: Mutex::new(HashMap::new()),
        }
    }

    fn record_duration(&self, cmd: &str, duration_ms: u64) {
        let mut durations = self.durations.lock().unwrap_or_else(|e| e.into_inner());
        durations
            .entry(cmd.to_string())
            .or_default()
            .push(duration_ms);
    }

    /// The median of this command's observed durations, `None` with no
    /// history yet.
    fn median_duration(&self, cmd: &str) -> Option<u64> {
        let durations = self.durations.lock().unwrap_or_else(|e| e.into_inner());
        let samples = durations.get(cmd)?;
        let mut sorted = samples.clone();
        sorted.sort_unstable();
        Some(sorted[sorted.len() / 2])
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
    /// DI-11: the dispatch's own `CancellationToken` fired — a
    /// `join: any` sibling won, or the user cancelled the run. The
    /// session was cut (interrupt→kill); the *caller* decides what the
    /// cancellation means, because only it knows which token fired.
    Cancelled,
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
    Blocked {
        reason: String,
    },
    /// DI-11: the cycle's cancellation token fired mid-attempt — the
    /// session was cut (interrupt→kill) and the cycle stopped without a
    /// verdict. What that means for the task's status is the caller's
    /// call, not this cycle's.
    Interrupted,
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

/// Runs one criterion command, measuring its wall-clock cost (DI-15) —
/// an observed fact about an external process, same standing as its
/// exit code; the injected `Clock` governs event timestamps and derived
/// state, neither of which this feeds.
async fn run_criterion(
    task_id: &TaskId,
    cwd: &Path,
    cmd: &str,
) -> Result<(i32, u64), TaskCycleError> {
    let started = std::time::Instant::now();
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
    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok((status.code().unwrap_or(-1), duration_ms))
}

/// Tree hash computed once per call and shared across every criterion in
/// it (§5.4) — criteria are read-only, so the tree can't change between
/// them, and one `git` round-trip beats N. `criteria` arrives already in
/// the order the caller wants executed (declared, or DI-15's learned
/// order) — this function only runs and records.
async fn run_all_criteria(
    task_id: &TaskId,
    criteria: &[Criterion],
    cwd: &Path,
    memo: &Memo,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    let tree_hash = tree_hash(cwd).await?;
    let mut runs = Vec::with_capacity(criteria.len());
    for criterion in criteria {
        let (exit_code, reused, duration_ms) = match memo.get(&criterion.cmd, &tree_hash) {
            Some(exit_code) => (exit_code, true, None),
            None => {
                let (exit_code, duration_ms) = run_criterion(task_id, cwd, &criterion.cmd).await?;
                memo.put(&criterion.cmd, &tree_hash, exit_code);
                memo.record_duration(&criterion.cmd, duration_ms);
                (exit_code, false, Some(duration_ms))
            }
        };
        runs.push(CriterionRun {
            cmd: criterion.cmd.clone(),
            exit_code,
            is_guard: criterion.r#type == Some(CriterionType::Guard),
            reused,
            duration_ms,
        });
    }
    Ok(runs)
}

/// Pre-check in rojo (§5.2 step 2): every non-`guard` criterion must
/// fail, every `guard` must pass. Runs every criterion regardless — the
/// report should show all of them, not stop at the first surprise.
/// Execution order is D62's learned one (DI-15): ascending historical
/// median duration, criteria without history last in declared order —
/// the fast, likely-to-fail evidence lands first while the verdict
/// (computed over the complete set) stays order-independent by
/// construction.
pub async fn pre_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
) -> Result<(Vec<CriterionRun>, PreCheckOutcome), TaskCycleError> {
    let mut ordered: Vec<&Criterion> = task.criteria.iter().collect();
    // Stable sort: no-history criteria (u64::MAX key) keep declared
    // order among themselves.
    ordered.sort_by_key(|criterion| memo.median_duration(&criterion.cmd).unwrap_or(u64::MAX));
    let ordered: Vec<Criterion> = ordered.into_iter().cloned().collect();
    let runs = run_all_criteria(&task.id, &ordered, cwd, memo).await?;

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
    run_all_criteria(&task.id, &task.criteria, cwd, memo).await
}

/// Everything about *how* one node's sessions open (DI-13), resolved
/// once by the engine and threaded through the cycle: the mounted
/// skills, the adapter's opaque settings, and the env — which is ONLY
/// the declared secret names present in the engine's own environment
/// (I12: values never touch the log, nothing undeclared leaks).
#[derive(Debug, Clone, Default)]
pub struct SessionSetup {
    pub skills: Vec<PathBuf>,
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    pub env: std::collections::HashMap<String, String>,
}

impl SessionSetup {
    /// The env a session may see (I12): declared names, present values.
    pub fn secrets_env(
        config: &yunta_core::ConfigLayer,
    ) -> std::collections::HashMap<String, String> {
        config
            .secrets
            .iter()
            .filter_map(|name| std::env::var(name).ok().map(|value| (name.clone(), value)))
            .collect()
    }
}

/// What a session dispatch needs from its surrounding run (DI-08/DI-09),
/// abstracted so `run_task` stays callable without a full run context
/// (its own integration tests): append the session's audit events, and
/// expose the process registry for pgid bookkeeping. `RunCtx` is the one
/// real implementor.
pub trait SessionObserver: Sync {
    fn emit_session_event(&self, node_id: &yunta_core::NodeId, payload: EventPayload);
    fn process_registry(&self) -> Option<&crate::process_registry::ProcessRegistry>;
}

/// The only shape of a note the log ever carries (I12/O3): its size and
/// a content-hash prefix — enough to audit a claimed note against,
/// never enough to reconstruct or leak it.
fn note_summary(text: &str) -> String {
    let hash = yunta_core::sha256_hex(text.as_bytes());
    format!("{} bytes, sha256 {}", text.len(), &hash[..12])
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
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    resume: Option<&yunta_core::SessionId>,
) -> Result<(DispatchOutcome, TokenUsage), YuntaError> {
    let budget = request.budget;
    let requested_agent = request.agent.clone();
    // DI-23: `Some` continues an interrupted conversation instead of
    // opening a new one — the caller already verified the capability.
    let mut session = match resume {
        Some(session_id) => adapter.resume(session_id, request).await?,
        None => adapter.spawn(request).await?,
    };
    // DI-08: on the map for a separate `yunta cancel` while it lives.
    let _pgid_registration = crate::process_registry::register(
        audit.and_then(|(observer, _)| observer.process_registry()),
        session.pgid(),
    );
    // DI-09: the session's own audit trail (`agent_session_opened` /
    // `agent_message`), emitted as the stream arrives so a concurrent
    // `status` sees the live session. A failed append warns instead of
    // aborting the stream — the run's next mandatory event hits the same
    // storage and fails the run properly if it's really down.
    let audit_emit = |payload: yunta_core::events::EventPayload| {
        if let Some((observer, node_id)) = audit {
            observer.emit_session_event(node_id, payload);
        }
    };

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
                AgentEvent::SessionOpened { session_id, model } => {
                    audit_emit(yunta_core::events::EventPayload::AgentSessionOpened(
                        yunta_core::events::AgentSessionOpenedPayload {
                            session_id,
                            agent: requested_agent.clone(),
                            model,
                            capabilities: adapter.capabilities(),
                        },
                    ));
                }
                AgentEvent::ToolUse {
                    name,
                    target_digest,
                } => {
                    audit_emit(yunta_core::events::EventPayload::AgentMessage(
                        yunta_core::events::AgentMessagePayload {
                            message_type: yunta_core::events::AgentMessageType::ToolUse,
                            tool_name: Some(name),
                            target_digest: Some(target_digest),
                            input_tokens: None,
                            output_tokens: None,
                            cached_input_tokens: None,
                            text: None,
                        },
                    ));
                }
                AgentEvent::Note { text } => {
                    audit_emit(yunta_core::events::EventPayload::AgentMessage(
                        yunta_core::events::AgentMessagePayload {
                            message_type: yunta_core::events::AgentMessageType::Note,
                            tool_name: None,
                            target_digest: None,
                            input_tokens: None,
                            output_tokens: None,
                            cached_input_tokens: None,
                            // I12/O3: a mechanical size+digest summary,
                            // never the content — the log must not be
                            // able to carry a secret the note contained.
                            text: Some(note_summary(&text)),
                        },
                    ));
                }
                AgentEvent::Usage {
                    input_tokens,
                    output_tokens,
                    cached_input_tokens,
                } => {
                    audit_emit(yunta_core::events::EventPayload::AgentMessage(
                        yunta_core::events::AgentMessagePayload {
                            message_type: yunta_core::events::AgentMessageType::Usage,
                            tool_name: None,
                            target_digest: None,
                            input_tokens: Some(input_tokens),
                            output_tokens: Some(output_tokens),
                            cached_input_tokens,
                            text: None,
                        },
                    ));
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
        return Ok((DispatchOutcome::Cancelled, tokens));
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
/// means the schema's own default, `deny`); `grants` is the batch's
/// shared [`crate::scope_expansion::GrantLedger`] (DI-16) — §6.2's
/// `max_per_run` is run-scoped, not task-scoped, and the ledger's
/// atomic cap window is what makes the count exact when several batch
/// members request at once.
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
    grants: &crate::scope_expansion::GrantLedger,
    already_granted_paths: &[String],
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    cancel: &CancellationToken,
    setup: &SessionSetup,
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
            env: setup.env.clone(),
            edit_constraints: Some(task.scope.clone()),
            budget,
            adapter_settings: setup.adapter_settings.clone(),
            skills: setup.skills.clone(),
        };
        let (dispatch_outcome, tokens) = dispatch_session(adapter, request, cancel, audit, None)
            .await
            .map_err(|source| TaskCycleError::Spawn {
                task: task.id.clone(),
                source,
            })?;

        // DI-11: a cancelled dispatch ends the cycle right here — no
        // post-check, no verdict, no retry. The attempt is on record;
        // what the cancellation means for the task is the caller's
        // decision, because only it knows which token fired.
        if matches!(dispatch_outcome, DispatchOutcome::Cancelled) {
            attempts.push(AttemptRecord {
                attempt,
                dispatch: dispatch_outcome,
                tokens,
                post_check: Vec::new(),
                scope: crate::scope::ScopeCheckResult::default(),
                succeeded: false,
                scope_expansion: None,
            });
            return Ok(TaskCycleReport {
                task_id: task.id.clone(),
                pre_check: pre_runs,
                attempts,
                outcome: TaskOutcome::Interrupted,
                needs_human_decision: false,
            });
        }

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
                        grants,
                        &expansion_request,
                        cwd,
                    )
                    .await
                    .map_err(|source| TaskCycleError::ScopeExpansion {
                        task: task.id.clone(),
                        source,
                    })?;
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
