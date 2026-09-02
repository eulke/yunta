//! The task cycle — the part of the ledger cycle
//! that actually runs a task through pre-check, dispatch, post-check and
//! scope check. `yunta_engine::register` validates a ledger before
//! any of this; this module is what happens once a task is `ready`.
//!
//! The engine, never the agent, decides `done`: [`run_task`] always
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
use yunta_core::events::{CapabilityDegradedPayload, CriterionType, EventPayload, TokenUsage};
use yunta_core::Criterion;
use yunta_core::{AdapterError, Task, TaskId};
use yunta_storage::StorageError;

use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome, Supervision};
use crate::scope::{scope_check, ScopeCheckError, ScopeCheckResult};

#[derive(Debug, Error)]
pub enum TaskCycleError {
    #[error("failed to run criterion `{cmd}` for task `{task}`")]
    Criterion {
        task: TaskId,
        cmd: String,
        #[source]
        source: crate::process::SpawnError,
    },
    #[error("adapter failed to spawn a session for task `{task}`")]
    Spawn {
        task: TaskId,
        #[source]
        source: AdapterError,
    },
    #[error("failed to append a session audit event for task `{task}`")]
    Audit {
        task: TaskId,
        #[source]
        source: StorageError,
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
    /// Whether this result came from the memoization cache instead of
    /// an actual execution — `criteria_checked` records it so recibo/replay
    /// show what ran versus what was reused, nothing verified in silence.
    pub reused: bool,
    /// Wall-clock milliseconds the execution took — what the
    /// learned ordering feeds on. `None` when `reused` (nothing ran).
    pub duration_ms: Option<u64>,
}

/// Per-run memoization cache: a criterion's result is reused when
/// its command, the working tree's content, and the resolved config are
/// all unchanged since the last time it ran *in this run*. Never
/// cross-run — a fresh `Memo` per `execute_run` call is correct, not a
/// gap: a resumed run simply starts with a cold cache and re-verifies
/// once more than strictly necessary, which is safe (over-verifying),
/// unlike a stale cross-run cache (which would risk under-verifying).
///
/// The full key is `cmd + tree_hash + declared env +
/// resolved config` — `declared env` drops out here because criteria
/// have no `env:` field in the schema yet (nothing to declare yet).
pub struct Memo {
    config_hash: String,
    cache: Mutex<HashMap<String, i32>>,
    /// Observed wall-clock durations per criterion command, this
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

    /// The median of this command's observed durations as a cheapest-first
    /// sort key, `None` with no history yet — the workspace's one
    /// [`crate::stats::median`], truncated back to whole milliseconds.
    fn median_duration(&self, cmd: &str) -> Option<u64> {
        let durations = self.durations.lock().unwrap_or_else(|e| e.into_inner());
        let samples = durations.get(cmd)?;
        let mut sorted: Vec<f64> = samples.iter().map(|&ms| ms as f64).collect();
        sorted.sort_by(|a, b| a.total_cmp(b));
        crate::stats::median(&sorted).map(|ms| ms as u64)
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

/// A fingerprint of `cwd`'s current content: the commit it's on,
/// its full diff against that commit (tracked changes), and every
/// untracked file's own content hash — conservative on purpose. Missing
/// an untracked file's content from the fingerprint would let two
/// genuinely different trees hash the same and wrongly reuse a stale
/// result; a bare filename list (from `git status`) isn't enough since a
/// file can change content without its name changing.
async fn tree_hash(cwd: &Path) -> Result<String, TaskCycleError> {
    let run_git = |args: &'static [&'static str]| async move {
        crate::git::output(cwd, args).await.map_err(|e| {
            let detail = e.detail();
            TaskCycleError::TreeHash {
                args: e.args,
                cwd: e.cwd,
                detail,
            }
        })
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

/// The pre-check's verdict: "esta fase valida al
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
    /// No terminal event at all — the engine synthesizes this, the
    /// adapter never emits it.
    Crashed,
    /// The dispatch's own `CancellationToken` fired — a
    /// `join: any` sibling won, or the user cancelled the run. The
    /// session was cut (interrupt→kill); the *caller* decides what the
    /// cancellation means, because only it knows which token fired.
    Cancelled,
    /// The engine cut the session via `interrupt` → `kill`:
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
    /// The agent's own expansion request this attempt, if
    /// it wrote one, and what the engine decided — `None` when no request
    /// file was found, the ordinary case. The caller (`loop_exec.rs`) owns
    /// emitting `scope_expansion_requested`/`granted`/`denied` and the
    /// finding conversion from this; `run_task` only decides and
    /// widens `scope` for this attempt's own check when granted.
    pub scope_expansion: Option<crate::scope_expansion::ScopeExpansionOutcome>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskOutcome {
    Done,
    Blocked {
        reason: String,
    },
    /// The cycle's cancellation token fired mid-attempt — the
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
    /// `true` when any attempt's scope-expansion request escalated (
    /// `ask` mode, or `max_per_run` already exhausted) — neither is a
    /// verdict `run_task` can render alone, so the cycle stops retrying
    /// and the caller (`loop_exec.rs`) puts the decision to
    /// `HumanInteraction` — pausing only when no live surface answers —
    /// rather than burning further sessions while one is owed.
    pub needs_human_decision: bool,
    /// What the adapter declared it wrote into the task's worktree for
    /// its own mechanics during the last attempt — what the scope
    /// check at integration leaves out, exactly as the cycle's own
    /// check did.
    pub staged: Vec<PathBuf>,
}

/// Default retry cap ("cap configurable, default 2").
pub const DEFAULT_MAX_RETRIES: u32 = 2;

/// Runs one criterion command, measuring its wall-clock cost —
/// an observed fact about an external process, same standing as its
/// exit code; the injected `Clock` governs event timestamps and derived
/// state, neither of which this feeds.
async fn run_criterion(
    task_id: &TaskId,
    cwd: &Path,
    cmd: &str,
    supervision: Supervision<'_>,
) -> Result<(i32, u64), TaskCycleError> {
    let started = std::time::Instant::now();
    // A criterion shares the engine's streams: its output is the
    // person's to read, its exit code the engine's to record.
    let command = GovernedCommand::shell(cwd, cmd)
        .stdout(Capture::Inherit)
        .stderr(Capture::Inherit);
    let exit_code = match spawn_governed(command, supervision)
        .await
        .map_err(|source| TaskCycleError::Criterion {
            task: task_id.clone(),
            cmd: cmd.to_string(),
            source,
        })? {
        Outcome::Exited { status, .. } => status.code().unwrap_or(-1),
        // Stopped by the engine before it could answer — never a real
        // exit code, so the record says so.
        Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => -2,
    };
    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok((exit_code, duration_ms))
}

/// Tree hash computed once per call and shared across every criterion in
/// it — criteria are read-only, so the tree can't change between
/// them, and one `git` round-trip beats N. `criteria` arrives already in
/// the order the caller wants executed (declared, or the learned
/// order) — this function only runs and records.
async fn run_all_criteria(
    task_id: &TaskId,
    criteria: &[Criterion],
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    let tree_hash = tree_hash(cwd).await?;
    let mut runs = Vec::with_capacity(criteria.len());
    for criterion in criteria {
        let (exit_code, reused, duration_ms) = match memo.get(&criterion.cmd, &tree_hash) {
            Some(exit_code) => (exit_code, true, None),
            None => {
                let (exit_code, duration_ms) =
                    run_criterion(task_id, cwd, &criterion.cmd, supervision).await?;
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

/// Pre-check in rojo: every non-`guard` criterion must
/// fail, every `guard` must pass. Runs every criterion regardless — the
/// report should show all of them, not stop at the first surprise.
/// Execution order is the learned one: ascending historical
/// median duration, criteria without history last in declared order —
/// the fast, likely-to-fail evidence lands first while the verdict
/// (computed over the complete set) stays order-independent by
/// construction.
pub async fn pre_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<(Vec<CriterionRun>, PreCheckOutcome), TaskCycleError> {
    let mut ordered: Vec<&Criterion> = task.criteria.iter().collect();
    // Stable sort: no-history criteria (u64::MAX key) keep declared
    // order among themselves.
    ordered.sort_by_key(|criterion| memo.median_duration(&criterion.cmd).unwrap_or(u64::MAX));
    let ordered: Vec<Criterion> = ordered.into_iter().cloned().collect();
    let runs = run_all_criteria(&task.id, &ordered, cwd, memo, supervision).await?;

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

/// Post-check: every criterion, guard or not, must now
/// pass.
pub async fn post_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    run_all_criteria(&task.id, &task.criteria, cwd, memo, supervision).await
}

/// Everything about *how* one node's sessions open, resolved
/// once by the engine and threaded through the cycle: the mounted
/// skills, the adapter's opaque settings, and the env — which is ONLY
/// the declared secret names present in the engine's own environment
/// (values never touch the log, nothing undeclared leaks).
#[derive(Clone, Default)]
pub struct SessionSetup {
    pub skills: Vec<PathBuf>,
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    pub env: std::collections::HashMap<String, yunta_core::Secret<String>>,
    /// The per-run MCP host plus the loop node's own id, present
    /// ONLY when the resolved adapter declared `run_tools` (the caller
    /// gates on the capability — this module never re-checks it). Each
    /// task attempt opens its own fresh listener+credential from it —
    /// per session, never reused.
    pub run_tools: Option<(
        std::sync::Arc<crate::run_tools::RunToolsHost>,
        yunta_core::NodeId,
    )>,
}

impl SessionSetup {
    /// The env a session may see: declared names, present values.
    pub fn secrets_env(
        config: &yunta_core::ConfigLayer,
    ) -> std::collections::HashMap<String, yunta_core::Secret<String>> {
        config
            .secrets
            .iter()
            .filter_map(|name| {
                std::env::var(name)
                    .ok()
                    .map(|value| (name.clone(), yunta_core::Secret::from(value)))
            })
            .collect()
    }
}

/// What a session dispatch needs from its surrounding run,
/// abstracted so `run_task` stays callable without a full run context
/// (its own integration tests): append the session's audit events, and
/// expose the process registry for pgid bookkeeping. `RunCtx` is the one
/// real implementor.
#[async_trait::async_trait]
pub trait SessionObserver: Sync {
    /// Appends one session audit event to the run's log. The storage
    /// cause travels back on failure so the dispatch fails the node
    /// rather than dropping the event — a lost audit event thins the
    /// trail `status` and replay read.
    async fn emit_session_event(
        &self,
        node_id: &yunta_core::NodeId,
        payload: EventPayload,
    ) -> Result<(), StorageError>;
    fn process_registry(&self) -> Option<&crate::process_registry::ProcessRegistry>;
}

/// How [`dispatch_session`] failed: the adapter refused, or a session
/// audit event could not be appended. The two have different owners —
/// the adapter boundary versus the run's own storage — so callers route
/// each to its own node/run failure.
#[derive(Debug, Error)]
pub enum DispatchError {
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error("failed to append a session audit event")]
    Audit(#[source] StorageError),
}

/// The only shape of a note the log ever carries: its size and
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
/// prompt-node execution: the request differs, the enforcement
/// does not.
///
/// The adapter passes `request.budget` along if its CLI supports it,
/// but enforcement is the engine's job either way — this counts `Usage`
/// and races the wall-clock deadline independent of that, and cuts the
/// session with `interrupt` → grace → `kill` when either budget is
/// exceeded.
pub(crate) async fn dispatch_session(
    adapter: &dyn Adapter,
    request: SessionRequest,
    cancel: &CancellationToken,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    resume: Option<&yunta_core::SessionId>,
) -> Result<(DispatchOutcome, TokenUsage), DispatchError> {
    let budget = request.budget;
    let requested_agent = request.agent.clone();
    // `Some` continues an interrupted conversation instead of
    // opening a new one — the caller already verified the capability.
    let mut session = match resume {
        Some(session_id) => adapter.resume(session_id, request).await?,
        None => adapter.spawn(request).await?,
    };
    // On the map for a separate `yunta cancel` while it lives.
    let _pgid_registration = crate::process_registry::register(
        audit.and_then(|(observer, _)| observer.process_registry()),
        session.pgid(),
    );
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

            if let Some(outcome) = apply_agent_event(AgentEventCtx {
                event,
                adapter,
                requested_agent: &requested_agent,
                max_tokens: budget.max_tokens,
                audit,
                tokens: &mut tokens,
            })
            .await?
            {
                terminal = Some(outcome);
                break;
            }
        }
    } // the stream's borrow of `session` ends here — interrupt/kill need &mut self too.

    if cancelled || matches!(terminal, Some(DispatchOutcome::BudgetExceeded { .. })) {
        // Never leave anything running: ordered termination first,
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

/// One streamed `AgentEvent` and the dispatch state [`apply_agent_event`]
/// folds it into — grouped so the read loop hands them over as a unit.
struct AgentEventCtx<'a> {
    event: AgentEvent,
    adapter: &'a dyn Adapter,
    requested_agent: &'a Option<yunta_core::AgentName>,
    max_tokens: Option<u64>,
    audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    tokens: &'a mut TokenUsage,
}

/// Appends one streamed event to the session's audit trail
/// (`agent_session_opened`/`agent_message`, emitted as the stream arrives so
/// a concurrent `status` sees the live session) and folds a `Usage` event
/// into the running token total. Returns the terminal outcome that ends the
/// stream — `Completed`, `Failed`, or a budget stop — or `None` to keep
/// reading. A failed audit append is never swallowed: its storage cause
/// ends the dispatch, so the node fails with the cause rather than the trail
/// losing an event nobody can recover.
async fn apply_agent_event(
    ctx: AgentEventCtx<'_>,
) -> Result<Option<DispatchOutcome>, DispatchError> {
    let AgentEventCtx {
        event,
        adapter,
        requested_agent,
        max_tokens,
        audit,
        tokens,
    } = ctx;
    match event {
        AgentEvent::SessionOpened { session_id, model } => {
            emit_audit(
                audit,
                EventPayload::AgentSessionOpened(yunta_core::events::AgentSessionOpenedPayload {
                    session_id,
                    agent: requested_agent.clone(),
                    model,
                    capabilities: adapter.capabilities(),
                }),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::ToolUse {
            name,
            target_digest,
        } => {
            emit_audit(
                audit,
                EventPayload::AgentMessage(yunta_core::events::AgentMessagePayload {
                    message_type: yunta_core::events::AgentMessageType::ToolUse,
                    tool_name: Some(name),
                    target_digest: Some(target_digest),
                    input_tokens: None,
                    output_tokens: None,
                    cached_input_tokens: None,
                    text: None,
                }),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::Note { text } => {
            emit_audit(
                audit,
                EventPayload::AgentMessage(yunta_core::events::AgentMessagePayload {
                    message_type: yunta_core::events::AgentMessageType::Note,
                    tool_name: None,
                    target_digest: None,
                    input_tokens: None,
                    output_tokens: None,
                    cached_input_tokens: None,
                    // A mechanical size+digest summary, never the content —
                    // the log must not be able to carry a secret the note
                    // contained.
                    text: Some(note_summary(&text)),
                }),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
        } => {
            emit_audit(
                audit,
                EventPayload::AgentMessage(yunta_core::events::AgentMessagePayload {
                    message_type: yunta_core::events::AgentMessageType::Usage,
                    tool_name: None,
                    target_digest: None,
                    input_tokens: Some(input_tokens),
                    output_tokens: Some(output_tokens),
                    cached_input_tokens,
                    text: None,
                }),
            )
            .await
            .map_err(DispatchError::Audit)?;
            tokens.input += input_tokens;
            tokens.output += output_tokens;
            if let Some(cached) = cached_input_tokens {
                tokens.cached = Some(tokens.cached.unwrap_or(0) + cached);
            }
            let tokens_used = tokens.total();
            if let Some(max_tokens) = max_tokens {
                if tokens_used > max_tokens {
                    return Ok(Some(DispatchOutcome::BudgetExceeded {
                        reason: format!("exceeded max_tokens {max_tokens} ({tokens_used} used)"),
                    }));
                }
            }
        }
        AgentEvent::Completed {
            result: AgentOutcome { summary },
        } => return Ok(Some(DispatchOutcome::Completed { summary })),
        AgentEvent::Failed { error, retryable } => {
            return Ok(Some(DispatchOutcome::Failed {
                message: error.message,
                retryable,
            }))
        }
    }
    Ok(None)
}

/// Appends one event to the session's audit trail, or does nothing when the
/// dispatch runs without an observer (a standalone `run_task` in a test).
async fn emit_audit(
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    payload: EventPayload,
) -> Result<(), StorageError> {
    match audit {
        Some((observer, node_id)) => observer.emit_session_event(node_id, payload).await,
        None => Ok(()),
    }
}

/// The permission/scope policy a task cycle enforces:
/// `permissions` is the merged model every criterion command is checked
/// against before anything runs — a violating criterion blocks the whole
/// task citing the rule (a policy outcome in the report, never an engine
/// abort), scanned here at the cycle's single entry point so the
/// standalone [`pre_check`]/[`post_check`] helpers stay pure building
/// blocks. `profile` is the node's own rung of the same permission
/// ladder, forwarded to every session this cycle opens. `scope_expansion`
/// carries the loop node's own settings (absent means the schema's
/// own default, `deny`); `grants` is the batch's shared
/// [`crate::scope_expansion::GrantLedger`] — `max_per_run` is
/// run-scoped, not task-scoped, and the ledger's atomic cap window is
/// what makes the count exact when several batch members request at
/// once. `already_granted_paths` are the paths every *prior*
/// `scope_expansion_granted` on the log authorized for this task — a
/// human grant lands between attempts, so the retry's effective scope
/// must include them from the very first diff it evaluates.
pub struct ScopeGovernance<'a> {
    pub permissions: Option<&'a yunta_core::PermissionsConfig>,
    pub profile: PermissionProfile,
    pub scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    /// The run's `limits.max_expansion_files` ceiling, resolved once by
    /// the caller — a `rules`-mode request touching more files than this
    /// is denied.
    pub max_expansion_files: usize,
    pub grants: &'a crate::scope_expansion::GrantLedger,
    pub already_granted_paths: &'a [String],
}

/// The resources and retry policy one task's attempts run under —
/// distinct from [`ScopeGovernance`] (what the session may touch) and
/// from the cross-cutting `audit`/`cancel`/`setup` surfaces (who's
/// watching and how it stops).
pub struct AttemptEnv<'a> {
    pub adapter: &'a dyn Adapter,
    pub cwd: &'a Path,
    pub max_retries: u32,
    pub budget: Budget,
    pub memo: &'a Memo,
    /// Where every criterion's process registers for the run.
    pub registry: Option<&'a crate::process_registry::ProcessRegistry>,
}

/// Runs a task through the full cycle: pre-check once, then
/// dispatch → post-check → scope-check per attempt, retrying with a
/// fresh session up to `max_retries` times before `Blocked`.
///
/// Never trusts the session's own outcome: `succeeded` on each
/// attempt is decided entirely by re-running criteria and the scope
/// diff, regardless of whether the session reported `Completed`.
#[tracing::instrument(
    skip_all,
    fields(
        task = %task.id,
        node_id = audit.map(|(_, node)| node.as_str()).unwrap_or_default(),
    )
)]
pub async fn run_task(
    task: &Task,
    instruction: &str,
    env: AttemptEnv<'_>,
    governance: ScopeGovernance<'_>,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    cancel: &CancellationToken,
    setup: &SessionSetup,
) -> Result<TaskCycleReport, TaskCycleError> {
    // What the adapter declares it stages, per attempt; nothing before
    // a session opens.
    let mut last_staged: Vec<PathBuf> = Vec::new();
    let AttemptEnv {
        adapter,
        cwd,
        max_retries,
        budget,
        memo,
        registry,
    } = env;
    let supervision = Supervision {
        registry,
        cancel: Some(cancel),
    };
    let ScopeGovernance {
        permissions,
        profile,
        scope_expansion,
        max_expansion_files,
        grants,
        already_granted_paths,
    } = governance;
    for criterion in &task.criteria {
        if let Some(rule) = crate::permissions::command_violation(&criterion.cmd, permissions) {
            return Ok(TaskCycleReport {
                task_id: task.id.clone(),
                staged: last_staged.clone(),
                pre_check: Vec::new(),
                attempts: Vec::new(),
                outcome: TaskOutcome::Blocked { reason: rule },
                needs_human_decision: false,
            });
        }
    }

    let (pre_runs, pre_outcome) = pre_check(task, cwd, memo, supervision).await?;

    // The pre-check validates the criteria before any work: a non-guard that
    // already passes, or a guard already red, means the criteria are wrong,
    // not the task. Only `Red` — nothing prejudged — proceeds to the
    // attempts; every other verdict blocks the task naming what to fix.
    let blocked_before_work = match pre_outcome {
        PreCheckOutcome::Red => None,
        PreCheckOutcome::TrivialCriterion { cmd } => Some(format!(
            "criterion `{cmd}` already passes before any work — the criteria need fixing, not the task"
        )),
        PreCheckOutcome::BrokenGuard { cmd } => {
            Some(format!("guard `{cmd}` is already red before any work started"))
        }
    };
    if let Some(reason) = blocked_before_work {
        return Ok(TaskCycleReport {
            task_id: task.id.clone(),
            staged: last_staged.clone(),
            pre_check: pre_runs,
            attempts: Vec::new(),
            outcome: TaskOutcome::Blocked { reason },
            needs_human_decision: false,
        });
    }

    let params = AttemptParams {
        task,
        instruction,
        adapter,
        cwd,
        budget,
        memo,
        profile,
        scope_expansion,
        max_expansion_files,
        grants,
        already_granted_paths,
        audit,
        cancel,
        setup,
        supervision,
    };
    let mut attempts = Vec::new();
    for attempt in 1..=(max_retries + 1) {
        let (staged, step) = run_one_attempt(&params, attempt).await?;
        last_staged = staged;
        match step {
            AttemptStep::Stop {
                record,
                outcome,
                needs_human_decision,
            } => {
                attempts.push(record);
                return Ok(TaskCycleReport {
                    task_id: task.id.clone(),
                    staged: last_staged,
                    pre_check: pre_runs,
                    attempts,
                    outcome,
                    needs_human_decision,
                });
            }
            AttemptStep::Again(record) => attempts.push(record),
        }
    }

    Ok(TaskCycleReport {
        task_id: task.id.clone(),
        staged: last_staged.clone(),
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

/// Everything one attempt of [`run_task`] reads: the per-cycle context that
/// never changes between attempts, so an attempt takes just this and its
/// number.
struct AttemptParams<'a> {
    task: &'a Task,
    instruction: &'a str,
    adapter: &'a dyn Adapter,
    cwd: &'a Path,
    budget: Budget,
    memo: &'a Memo,
    profile: PermissionProfile,
    scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    max_expansion_files: usize,
    grants: &'a crate::scope_expansion::GrantLedger,
    already_granted_paths: &'a [String],
    audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    cancel: &'a CancellationToken,
    setup: &'a SessionSetup,
    supervision: Supervision<'a>,
}

/// What one attempt tells [`run_task`] to do next.
enum AttemptStep {
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
async fn run_one_attempt(
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
        Some((host, node_id)) => match crate::run_tools::open_session_listener(
            host.clone(),
            node_id.clone(),
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
                                capability: "run_tools".to_string(),
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
        run_tools_endpoint: run_tools.as_ref().map(|session| session.endpoint.clone()),
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
