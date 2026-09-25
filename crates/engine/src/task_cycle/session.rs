//! One agent session for a task: opening it, folding its events into
//! the log as they arrive, and closing it when the attempt ends or is
//! interrupted.

use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, TokenUsage};
use yunta_core::port::{Adapter, SessionRequest};
use yunta_core::AdapterError;
use yunta_storage::StorageError;

use super::DispatchOutcome;

/// Everything about *how* one node's sessions open, resolved
/// once by the engine and threaded through the cycle: the mounted
/// skills, the adapter's opaque settings, and the env — which is ONLY
/// the declared secret names present in the engine's own environment
/// (values never touch the log, nothing undeclared leaks).
#[derive(Clone)]
pub struct SessionSetup {
    pub skills: Vec<PathBuf>,
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    pub env: std::collections::HashMap<String, yunta_core::Secret<String>>,
    /// The per-run MCP host plus the loop node's own id, present
    /// ONLY when the resolved adapter declared `run_tools` (the caller
    /// gates on the capability — this module never re-checks it). Each
    /// task attempt opens its own fresh listener+credential from it —
    /// per session, never reused.
    pub run_tools: Option<crate::run_tools::RunToolsAccess>,
    /// The run's own directory, so a task session can be told where its
    /// adapter may drop scaffolding — outside the worktree, whose diff
    /// the scope check reads.
    pub run_dir: PathBuf,
    /// The loop node these task sessions belong to. It names each
    /// session's own scratch directory, so concurrent attempts of one
    /// node never write over each other's scaffolding.
    pub node: yunta_core::NodeId,
    /// The runner this node resolved to: every task session of the node
    /// runs on its model and, when it names one, its agent. The runner
    /// is resolved once for the whole loop, so no attempt can drift onto
    /// another model than the one the log recorded.
    pub chosen: yunta_core::RunnerCandidate,
    /// Where the files this node declares belong, when it declares any.
    /// A task session writes the loop node's own artifacts, so it is
    /// told the same directory the node closes on.
    pub artifact_dir: Option<PathBuf>,
    /// What makes this node's run tools mandatory rather than an offer:
    /// a `coordination: blackboard` group whose semantics the engine
    /// never emulates, or an interpreted artifact that has no other way
    /// in. A listener that fails to bind for such a node fails the node;
    /// for any other node it degrades and the session runs on.
    pub run_tools_required: Option<RunToolsNeed>,
    /// The hook a CLI runs to ask the judge about one write. `None` in a
    /// harness with no binary to run; an adapter whose fence needs it
    /// and does not have it fails the session.
    pub fence_hook: Option<yunta_core::fence::FenceHook>,
}

/// Why a node cannot proceed without the run tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunToolsNeed {
    /// The node is in a `coordination: blackboard` group.
    Blackboard,
    /// The node declares an interpreted artifact a session hands over
    /// through the tools.
    TypedArtifact(yunta_core::ArtifactKind),
}

impl SessionSetup {
    /// A setup that carries nothing but the run it belongs to, the node
    /// its sessions are of and the runner they run on: no skills, no
    /// settings, no secrets, no per-run tools and no declared files.
    ///
    /// The run directory is not among what a bare setup leaves out: a
    /// session writes its working files under it, and a path that names
    /// nowhere is one every such write resolves against the current
    /// directory instead.
    pub fn bare(
        run_dir: PathBuf,
        node: yunta_core::NodeId,
        chosen: yunta_core::RunnerCandidate,
    ) -> Self {
        Self {
            skills: Vec::new(),
            adapter_settings: serde_json::Map::new(),
            env: std::collections::HashMap::new(),
            run_tools: None,
            fence_hook: None,
            run_dir,
            node,
            chosen,
            artifact_dir: None,
            run_tools_required: None,
        }
    }

    /// The env a session may see: the names the config declares, bound
    /// to whatever `source` has for them. A name nothing binds simply
    /// does not reach the session — a secret the run cannot produce is
    /// absent, never empty.
    pub fn secrets_env(
        config: &yunta_core::ConfigLayer,
        source: Option<&dyn yunta_core::SecretSource>,
    ) -> std::collections::HashMap<String, yunta_core::Secret<String>> {
        let Some(source) = source else {
            return std::collections::HashMap::new();
        };
        config
            .secrets
            .iter()
            .filter_map(|name| source.get(name).map(|value| (name.clone(), value)))
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
pub(super) fn note_summary(text: &str) -> String {
    let hash = yunta_core::sha256_hex(text.as_bytes()).to_string();
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
/// What one session left behind: how it ended, what it spent, and how
/// much of its writes its adapter's fence covered — a cache of one
/// invocation of what the log already carries.
pub(crate) struct Dispatched {
    pub outcome: DispatchOutcome,
    pub tokens: TokenUsage,
    pub fence: Option<yunta_core::fence::Coverage>,
}

pub(crate) async fn dispatch_session(
    adapter: &dyn Adapter,
    request: SessionRequest,
    cancel: &CancellationToken,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    resume: Option<&yunta_core::SessionId>,
) -> Result<Dispatched, DispatchError> {
    let budget = request.budget;
    let requested_agent = request.agent.clone();
    // Whether this session was handed a per-run tool server at all: a
    // session that holds none of those tools only means something went
    // wrong if it was given a server to hold them from.
    let run_tools_offered = request.run_tools_endpoint.is_some();
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
    let mut opened: Option<yunta_core::SessionId> = None;
    let mut fence: Option<yunta_core::fence::Coverage> = None;
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

            if let Some(outcome) = crate::task_cycle::stream::apply_agent_event(
                crate::task_cycle::stream::AgentEventCtx {
                    event,
                    adapter,
                    requested_agent: &requested_agent,
                    run_tools_offered,
                    max_tokens: budget.max_tokens,
                    audit,
                    tokens: &mut tokens,
                    opened: &mut opened,
                    fence: &mut fence,
                },
            )
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
        return Ok(Dispatched {
            outcome: DispatchOutcome::Cancelled,
            tokens,
            fence,
        });
    }

    let outcome = match terminal {
        Some(outcome) => outcome,
        // Only the one that fell silent is asked: a session that closed
        // its turn said everything it had to say, and asking it would
        // cost a kill and a wait for nothing.
        None => DispatchOutcome::Crashed {
            exit: session.exit().await?,
        },
    };
    Ok(Dispatched {
        outcome,
        tokens,
        fence,
    })
}
