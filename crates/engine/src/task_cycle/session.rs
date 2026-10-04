//! One agent session for a task: opening it, folding its events into
//! the log as they arrive, and closing it when the attempt ends or is
//! interrupted.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, SessionEvent, TokenUsage};
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
    /// What no session of the run may write: what the project denies to
    /// every run, and every test a person approved. Every session's fence
    /// refuses it, whatever its scope.
    pub denied: Vec<yunta_core::ScopeGlob>,
    /// The directories every session of the run shares, which a session
    /// that may write keeps writable beside its checkout.
    pub shared_dirs: Vec<PathBuf>,
    /// The plan a loop's tasks come from, which each of its task
    /// sessions reads its place in. `None` for a node that works no
    /// tasks.
    pub plan: Option<std::sync::Arc<yunta_core::TasksFile>>,
    /// The tests the plan's tasks are held to, when the run holds a spec:
    /// each task's files are laid over the tree its work starts from and
    /// denied to that work.
    pub spec: Option<std::sync::Arc<yunta_core::SpecFile>>,
    /// The suite the run measured green before any work, which holds
    /// every task as a guard. `None` when it holds none.
    pub suite: Option<String>,
    /// The files a spec the run accepted before gave each task, which the
    /// spec it holds now does not: work the task did under the earlier
    /// spec carries them, and they leave it before the current ones go in.
    pub superseded: std::collections::BTreeMap<yunta_core::TaskId, Vec<PathBuf>>,
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
    /// never emulates, an interpreted artifact that has no other way in,
    /// or a loop whose task sessions read their task nowhere else. A
    /// listener that fails to bind for such a node fails the node; for
    /// any other node it degrades and the session runs on.
    pub run_tools_required: Option<RunToolsNeed>,
    /// The hook a CLI runs to ask the judge about one write. `None` in a
    /// harness with no binary to run; an adapter whose fence needs it
    /// and does not have it fails the session.
    pub fence_hook: Option<yunta_core::fence::FenceHook>,
    /// What a node's own session works to: the scope the node declared
    /// plus every path a person granted it on this run, which fences the
    /// session and is what its scope tools judge by. `None` for a node
    /// that declares no scope, and for a read-only one, whose profile is
    /// its whole ceiling. A task session works to its task's scope
    /// instead.
    pub node_scope: Option<Arc<crate::run_tools::NodeScopeAccess>>,
}

/// Why a node cannot proceed without the run tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunToolsNeed {
    /// The node is in a `coordination: blackboard` group.
    Blackboard,
    /// The node declares an interpreted artifact a session hands over
    /// through the tools.
    TypedArtifact(yunta_core::ArtifactKind),
    /// The node is a loop: its task sessions read their task and check
    /// their work through the tools, and nowhere else.
    Task,
}

impl RunToolsNeed {
    /// The reason `node` owes, if any — one answer for `check`, which
    /// refuses a workflow no candidate adapter can serve, and for the
    /// run, which refuses the session its adapter cannot: a member of a
    /// blackboard group first, being the older reason, then a document it
    /// declares, then the loop it is.
    pub(crate) fn of(
        node: &yunta_core::Node,
        blackboard_member: bool,
        declared: &[yunta_core::ArtifactSpec],
    ) -> Option<Self> {
        if blackboard_member {
            return Some(RunToolsNeed::Blackboard);
        }
        if let Some(kind) = declared.iter().find_map(yunta_core::ArtifactSpec::kind) {
            return Some(RunToolsNeed::TypedArtifact(kind));
        }
        matches!(node.kind, yunta_core::NodeKind::Loop { .. }).then_some(RunToolsNeed::Task)
    }

    /// The declaration that asks for them, as the workflow spells it.
    pub(crate) fn declaration(&self) -> String {
        match self {
            RunToolsNeed::Blackboard => "coordination: blackboard".to_string(),
            RunToolsNeed::TypedArtifact(kind) => format!("artifacts.produces: [{kind}]"),
            RunToolsNeed::Task => "kind: loop".to_string(),
        }
    }
}

impl SessionSetup {
    /// A setup that carries nothing but the run it belongs to, the node
    /// its sessions are of and the runner they run on: no skills, no
    /// settings, no secrets, no per-run tools, no declared files and no
    /// denied paths.
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
            denied: Vec::new(),
            shared_dirs: Vec::new(),
            plan: None,
            spec: None,
            suite: None,
            superseded: Default::default(),
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
            node_scope: None,
        }
    }

    /// The env a session may see: each shared directory under its
    /// variable, and the secret names the config declares, bound to
    /// whatever `source` has for them. A name nothing binds simply does
    /// not reach the session — a secret the run cannot produce is
    /// absent, never empty.
    pub fn secrets_env(
        config: &yunta_core::ConfigLayer,
        source: Option<&dyn yunta_core::SecretSource>,
    ) -> std::collections::HashMap<String, yunta_core::Secret<String>> {
        let shared = config
            .shared_dirs()
            .map(|(var, dir)| (var.to_string(), dir.display().to_string().into()));
        let secrets = source.into_iter().flat_map(|source| {
            config
                .secrets
                .iter()
                .filter_map(|name| source.get(name).map(|value| (name.clone(), value)))
        });
        shared.chain(secrets).collect()
    }
}

/// What a task cycle needs from its surrounding run,
/// abstracted so `run_task` stays callable without a full run context
/// (its own integration tests): append what the cycle observes — each
/// session's audit events, and each check the moment it runs — and
/// expose the process registry for pgid bookkeeping. `RunCtx` is the one
/// real implementor.
#[async_trait::async_trait]
pub trait SessionObserver: Sync {
    /// Appends one event to the run's log and returns the sequence
    /// number the log gave it. The storage cause travels back on failure
    /// so the cycle fails the node rather than dropping the event — a
    /// lost event thins the trail `status`, replay and a task session's
    /// own tools read.
    async fn record(
        &self,
        node_id: &yunta_core::NodeId,
        payload: EventPayload,
    ) -> Result<yunta_core::Seq, StorageError>;
    /// Keeps what a command printed where the run keeps every object it
    /// holds, with the run's secrets taken out, and answers the hash it
    /// is named by. What a cycle's commands print is the run's, never
    /// the terminal's.
    async fn keep_output(
        &self,
        output: &crate::process::CommandOutput,
    ) -> std::io::Result<yunta_core::ContentHash>;
    fn process_registry(&self) -> Option<&crate::process_registry::ProcessRegistry>;
    /// Waits until the host the run works on has stayed awake a while
    /// since it last slept — `false` when `cancel` fired first. An
    /// observer that watches no host answers at once.
    async fn host_settled(&self, _cancel: &CancellationToken) -> Result<bool, StorageError> {
        Ok(true)
    }
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

/// The most a session gets between `interrupt` and the follow-up `kill`
/// once a budget has been exceeded — long enough for a session that
/// closes cleanly on interrupt to actually do so, short enough that a
/// session that ignores it doesn't stall the attempt. One that leaves
/// sooner is killed — what is left of its group — the moment it does.
const INTERRUPT_GRACE_PERIOD: Duration = Duration::from_millis(200);

/// Spawns one session from `request`, drains it to a terminal outcome
/// and reports the tokens it consumed. Shared by the task cycle and by
/// prompt-node execution: the request differs, the enforcement
/// does not.
///
/// The adapter passes `request.budget` along if its CLI supports it,
/// but enforcement is the engine's job either way — this counts `Usage`
/// and races the timeout — time the host is awake — independent of
/// that, and cuts the
/// session with `interrupt` → grace → `kill` when either budget is
/// exceeded.
/// What one session left behind: how it ended, what it spent, and how
/// much of its writes its adapter's fence covered — a cache of one
/// invocation of what the log already carries.
pub(crate) struct Dispatched {
    pub outcome: DispatchOutcome,
    pub tokens: TokenUsage,
    pub fence: Option<yunta_core::fence::Coverage>,
    /// The session the stream opened, once it did: what the next
    /// attempt resumes when something it asked for changes.
    pub session: Option<yunta_core::SessionId>,
}

/// How a session opens: the task it works, if any, and the conversation
/// it continues, if any.
#[derive(Clone, Copy, Default)]
pub(crate) struct Opening<'a> {
    pub(crate) task: Option<&'a yunta_core::TaskId>,
    pub(crate) resume: Option<Resume<'a>>,
}

/// A conversation to pick back up, and what a fresh session is told
/// instead when the adapter cannot pick it up — `None` when there is no
/// such way back and not resuming is the caller's failure to report.
#[derive(Clone, Copy)]
pub(crate) struct Resume<'a> {
    pub(crate) session: &'a yunta_core::SessionId,
    pub(crate) fresh_prompt: Option<&'a str>,
}

pub(crate) async fn dispatch_session(
    adapter: &dyn Adapter,
    request: SessionRequest,
    cancel: &CancellationToken,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    opening: Opening<'_>,
) -> Result<Dispatched, DispatchError> {
    // A host that just woke may sleep again within the minute, and a
    // session opened then hangs until its timeout: none opens until the
    // host has stayed awake.
    if let Some((observer, _)) = audit {
        if !observer
            .host_settled(cancel)
            .await
            .map_err(DispatchError::Audit)?
        {
            return Ok(Dispatched {
                outcome: DispatchOutcome::Cancelled,
                tokens: TokenUsage::default(),
                fence: None,
                session: None,
            });
        }
    }
    let budget = request.budget;
    let requested_agent = request.agent.clone();
    // Whether this session was handed a per-run tool server at all: a
    // session that holds none of those tools only means something went
    // wrong if it was given a server to hold them from.
    let run_tools_offered = request.run_tools_endpoint.is_some();
    // A resume continues a conversation instead of opening a new one. One
    // the adapter cannot pick up — it declares no resume, or its CLI
    // refuses this one — opens fresh on what the caller gave for that, and
    // the log says so; with nothing given, the caller already verified
    // the capability and not resuming is its failure to report.
    // What the adapter staged for this session is taken back once it
    // ends, which needs the request the opening consumes.
    let staging = request.clone();
    let (mut session, continues) = match opening.resume {
        None => (adapter.spawn(request).await?, None),
        Some(resume) => {
            let can_resume = adapter
                .capabilities()
                .declares(yunta_core::Capability::ResumeSession);
            let resumed = match (resume.fresh_prompt, can_resume) {
                (Some(_), false) => None,
                _ => Some(adapter.resume(resume.session, request.clone()).await),
            };
            match resumed {
                Some(Ok(session)) => (session, Some(resume.session)),
                Some(Err(error)) if resume.fresh_prompt.is_none() => return Err(error.into()),
                _ => {
                    let brief = resume.fresh_prompt.unwrap_or_default();
                    crate::task_cycle::stream::emit_audit(
                        audit,
                        EventPayload::Session(SessionEvent::CapabilityDegraded(
                            yunta_core::events::CapabilityDegradedPayload::new(
                                yunta_core::Capability::ResumeSession,
                                adapter.id().clone(),
                                yunta_core::events::Policy::FreshSession,
                            ),
                        )),
                    )
                    .await
                    .map_err(DispatchError::Audit)?;
                    let fresh = crate::run::session_plan::with_prompt(request, brief.to_string());
                    (adapter.spawn(fresh).await?, None)
                }
            }
        }
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
                                    reason: format!(
                                        "exceeded timeout of {}",
                                        yunta_core::units::duration(timeout)
                                    ),
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
                    task: opening.task,
                    continues,
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
        session.allow_exit(INTERRUPT_GRACE_PERIOD).await;
        let _ = session.kill().await;
    }
    adapter.unstage(&staging)?;

    if cancelled {
        return Ok(Dispatched {
            outcome: DispatchOutcome::Cancelled,
            tokens,
            fence,
            session: opened,
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
        session: opened,
    })
}
