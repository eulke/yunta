//! One agent session for a task: opening it, folding its events into
//! the log as they arrive, and closing it when the attempt ends or is
//! interrupted.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, SessionEvent, TokenUsage};
use yunta_core::port::{Adapter, AgentSession, SessionRequest};
use yunta_core::protected::Protected;
use yunta_storage::StorageError;

use super::dispatch::{read_and_end, DispatchError, Dispatched, Opening, Resume};
use super::stream::{counts_tokens_mid_session, emit_audit, record_rail_summary, Terms, Totals};
use super::{DispatchOutcome, RunToolsNeed};

/// Everything about *how* one node's sessions open, resolved
/// once by the engine and threaded through the cycle: the mounted
/// skills, the adapter's opaque settings, and the env — the shared
/// directories, and the declared secrets the engine's own environment
/// binds (values never touch the log, nothing undeclared leaks).
#[derive(Clone)]
pub struct SessionSetup {
    /// What no session of the run may write, whatever its scope allows:
    /// the run's protected set. Every session's fence refuses it.
    pub protected: Arc<Protected>,
    /// The git directory the run's repository keeps its register in, which
    /// no session writes. `None` when the run does not know its repository.
    pub git_register: Option<PathBuf>,
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
    /// The commands the run measured green before any work — its suite,
    /// and each `baseline.held` names — which hold every task as guards.
    pub held: Vec<String>,
    /// The files a spec the run accepted before gave each task, which the
    /// spec it holds now does not: work the task did under the earlier
    /// spec carries them, and they leave it before the current ones go in.
    pub superseded: std::collections::BTreeMap<yunta_core::TaskId, Vec<PathBuf>>,
    pub skills: Vec<PathBuf>,
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    pub env: yunta_core::port::session::SessionEnv,
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
    /// The hook a CLI runs to ask the engine about one write. `None` leaves
    /// an adapter whose fence needs a hook without one, and it fails the
    /// session; a harness whose sessions never run the hook names it anyway.
    pub rail_command: Option<yunta_core::rail::RailCommand>,
    /// The agent this node's sessions work as, resolved from the name the
    /// runner or the node gave.
    pub agent: Option<yunta_core::agent::AgentDefinition>,
    /// Whose configuration of the CLI this node's sessions load beside
    /// what the run mounts.
    pub cli_config: yunta_core::CliConfig,
    /// Whether this node's sessions' commands may reach the network, as
    /// `network_asked` resolves it: the node's own word, else the project's
    /// default, else open. What a session reaches is decided by its
    /// confinement, which refuses an unconfined session with its network closed.
    pub network: bool,
    /// What a node's own session works to: the scope the node declared
    /// plus every path a person granted it on this run, which fences the
    /// session and is what its scope tools judge by. `None` for a node
    /// that declares no scope, and for a read-only one, whose profile is
    /// its whole ceiling. A task session works to its task's scope
    /// instead.
    pub node_scope: Option<Arc<crate::run_tools::NodeScopeAccess>>,
}

impl SessionSetup {
    /// A setup that carries nothing but the run it belongs to, the node
    /// its sessions are of and the runner they run on: no skills, no
    /// settings, no secrets, no per-run tools, no declared files and no
    /// protected paths. Its hook is named `yunta`, as a harness names it,
    /// and no session of a harness runs it.
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
            protected: Arc::new(Protected::default()),
            git_register: None,
            shared_dirs: Vec::new(),
            plan: None,
            spec: None,
            held: Vec::new(),
            superseded: Default::default(),
            skills: Vec::new(),
            adapter_settings: serde_json::Map::new(),
            env: yunta_core::port::session::SessionEnv::default(),
            run_tools: None,
            rail_command: Some(yunta_core::rail::RailCommand::new(PathBuf::from("yunta"))),
            agent: None,
            cli_config: yunta_core::CliConfig::Inherit,
            network: yunta_core::network_asked(None, None),
            run_dir,
            node,
            chosen,
            artifact_dir: None,
            run_tools_required: None,
            node_scope: None,
        }
    }

    /// The env a session may see: each shared directory under its
    /// variable, as plain text, and each name the config declares under
    /// `secrets:` that `source` binds, as a secret under that name. The
    /// credentials the engine itself uses (an MCP server's bearer, the
    /// forge token) stay out of a session, which never receives them. A
    /// name nothing binds simply does not reach the session — a secret the
    /// run cannot produce is absent, never empty.
    pub fn secrets_env(
        config: &yunta_core::ConfigLayer,
        source: &dyn yunta_core::SecretSource,
    ) -> yunta_core::port::session::SessionEnv {
        yunta_core::port::session::SessionEnv {
            plain: config
                .shared_dirs()
                .map(|(var, dir)| (var.to_string(), dir.display().to_string()))
                .collect(),
            secret: config
                .secrets
                .iter()
                .filter_map(|name| source.get(name).map(|value| (name.clone(), value)))
                .collect(),
        }
    }
}

/// What a task cycle needs from its surrounding run,
/// abstracted so `run_task` stays callable without a full run context
/// (its own integration tests): append what the cycle observes — each
/// session's audit events, and each check the moment it runs — and
/// expose the process registry for pgid bookkeeping. `RunCtx` is the one
/// real implementor.
#[async_trait::async_trait]
pub trait SessionObserver: KeepsOutput {
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
    /// Every write these sessions were refused, in the order the log holds
    /// them: what an attempt reads its refusals from.
    async fn refused_writes(
        &self,
        sessions: &[yunta_core::SessionId],
    ) -> Result<Vec<yunta_core::events::ToolTarget>, StorageError>;
    fn process_registry(&self) -> Option<&crate::process_registry::ProcessRegistry>;
    /// Waits until the host the run works on has stayed awake a while
    /// since it last slept — `false` when `cancel` fired first. An
    /// observer that watches no host answers at once.
    async fn host_settled(&self, _cancel: &CancellationToken) -> Result<bool, StorageError> {
        Ok(true)
    }
    /// The run's reading of the time the host has been awake — what a
    /// wait that must not count the host's sleep measures itself by.
    fn awake(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
    /// What the shell had for the variables adapters reach their service
    /// by, which every session's CLI inherits. Default: nothing captured.
    fn shell_service(&self) -> Option<&std::collections::BTreeMap<String, String>> {
        None
    }
}

/// Where a run keeps what its commands printed: every object it holds,
/// with the run's secrets taken out. What a check's commands print is the
/// run's, never the terminal's.
#[async_trait::async_trait]
pub trait KeepsOutput: Sync {
    /// Keeps `output` and answers the hash it is named by.
    async fn keep_output(
        &self,
        output: &crate::process::CommandOutput,
    ) -> std::io::Result<yunta_core::ContentHash>;

    /// What was kept under `hash` — `None` when it cannot be read.
    async fn kept_output(&self, hash: &yunta_core::ContentHash) -> Option<Vec<u8>>;
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

/// Spawns one session from `request`, drains it to a terminal outcome and
/// reports what it spent — and whether it ended because it lost the
/// service behind it.
///
/// The adapter passes `request.budget` along if its CLI supports it, but
/// enforcement is the engine's job either way: this counts `Usage` and
/// races the timeout — time the host is awake — independent of that, and
/// cuts the session with `interrupt` → grace → `kill` when either budget
/// is exceeded.
pub(super) async fn stream_session(
    adapter: &dyn Adapter,
    request: SessionRequest,
    cancel: &CancellationToken,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    opening: Opening<'_>,
) -> Result<(Dispatched, bool), DispatchError> {
    // A host that just woke may sleep again within the minute, and a
    // session opened then hangs until its timeout: none opens until the
    // host has stayed awake.
    if !host_settled(audit, cancel).await? {
        let nothing = Dispatched {
            outcome: DispatchOutcome::Cancelled,
            tokens: TokenUsage::default(),
            fence: None,
            confinement: None,
            sessions: Vec::new(),
        };
        return Ok((nothing, false));
    }
    let terms = Terms {
        adapter,
        audit,
        agent: request.agent.as_ref().map(|agent| agent.name.clone()),
        effort: request.effort,
        budget: request.budget,
        confinement: yunta_core::events::ConfinementRecord::of(&request.confinement),
        counts_cap_mid_session: counts_tokens_mid_session(adapter),
    };
    record_cap_at_session_end(&terms).await?;
    let (mut session, continues) = spawn_or_resume(adapter, request, audit, opening.resume).await?;
    // On the map for a separate `yunta cancel` while it lives.
    let registration = crate::process_registry::register(
        audit.and_then(|(observer, _)| observer.process_registry()),
        session.pgid(),
    );
    let drained = drain(&mut *session, cancel, &terms, opening, continues).await;
    // A group the engine could not stop may still run, so it stays on the map.
    if let (Some(registration), Err(DispatchError::NotStopped { .. })) = (registration, &drained) {
        registration.keep();
    }
    drained
}

/// Reads an opened session to its terminal outcome, releases it, and writes
/// the summary of its rail's calls once the outcome is decided. Every way the
/// read ends passes through that write.
async fn drain(
    session: &mut dyn AgentSession,
    cancel: &CancellationToken,
    terms: &Terms<'_>,
    opening: Opening<'_>,
    continues: Option<&yunta_core::SessionId>,
) -> Result<(Dispatched, bool), DispatchError> {
    let drained = read_and_release(session, cancel, terms, opening, continues).await;
    record_rail_summary(terms, opening.rails).await;
    drained
}

/// Reads an opened session to its terminal outcome, ends it when the read was
/// cut short, and says what it spent and whether it ended on a lost service.
/// The session is released whatever the read and the end did.
async fn read_and_release(
    session: &mut dyn AgentSession,
    cancel: &CancellationToken,
    terms: &Terms<'_>,
    opening: Opening<'_>,
    continues: Option<&yunta_core::SessionId>,
) -> Result<(Dispatched, bool), DispatchError> {
    let mut totals = Totals::default();
    // The read, and the end of a read cut short, give one result; the session
    // is released whatever that result is.
    let ended = read_and_end(session, cancel, terms, opening, continues, &mut totals).await;
    let released = session.release().await;
    let (terminal, cancelled) = match (ended, released) {
        (Ok(read), Ok(())) => read,
        (Ok(_), Err(release)) => return Err(DispatchError::Release(release)),
        (Err(DispatchError::NotStopped { detail, tokens, .. }), Err(release)) => {
            return Err(DispatchError::NotStopped {
                detail,
                release: Some(Box::new(release)),
                tokens,
            })
        }
        (Err(error), Ok(())) => return Err(error),
        // The error that ended the session is the one reported; a release
        // that failed after it is only logged.
        (Err(error), Err(release)) => {
            tracing::debug!(%release, "a session's release failed after the session ended");
            return Err(error);
        }
    };
    let outcome = match (cancelled, terminal) {
        (true, _) => DispatchOutcome::Cancelled,
        (false, Some(outcome)) => outcome,
        // Only the one that fell silent is asked: a session that closed
        // its turn said everything it had to say, and asking it would
        // cost a kill and a wait for nothing.
        (false, None) => DispatchOutcome::Crashed {
            exit: session.exit().await?,
        },
    };
    let dispatched = Dispatched {
        outcome,
        tokens: totals.tokens,
        fence: totals.fence,
        confinement: totals.confinement,
        sessions: totals.opened.into_iter().collect(),
    };
    Ok((dispatched, totals.cut_off))
}

/// Waits until the host has stayed awake long enough to open a session on,
/// as the observer reads it. `false` when `cancel` fired first; an attempt
/// with no observer has no host to wait on.
async fn host_settled(
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    cancel: &CancellationToken,
) -> Result<bool, DispatchError> {
    match audit {
        Some((observer, _)) => observer
            .host_settled(cancel)
            .await
            .map_err(DispatchError::Audit),
        None => Ok(true),
    }
}

/// Records the degradation a session owes when its budget caps tokens and its
/// adapter does not count the cap mid-session: the cap is read once, when the
/// session closes. Recorded before the session opens, so the log says so
/// before the session can spend anything.
async fn record_cap_at_session_end(terms: &Terms<'_>) -> Result<(), DispatchError> {
    if terms.budget.max_tokens.is_none() || terms.counts_cap_mid_session {
        return Ok(());
    }
    let degraded = yunta_core::events::CapabilityDegradedPayload::new(
        yunta_core::Capability::MidSessionTokenCap,
        terms.adapter.id().clone(),
        yunta_core::port::Policy::CapAtSessionEnd,
    );
    emit_audit(
        terms.audit,
        EventPayload::Session(SessionEvent::CapabilityDegraded(degraded)),
    )
    .await
    .map_err(DispatchError::Audit)
}

/// Opens the session `request` describes, or picks back up the conversation
/// `resume` names when the adapter can. A conversation the adapter cannot
/// pick up opens fresh on the brief `resume` carries, and the log says so;
/// with no brief, the caller already verified the capability and not
/// resuming is its failure to report. Returns the session, and the
/// conversation it continues when it continues one.
async fn spawn_or_resume<'a>(
    adapter: &dyn Adapter,
    request: SessionRequest,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    resume: Option<Resume<'a>>,
) -> Result<(Box<dyn AgentSession>, Option<&'a yunta_core::SessionId>), DispatchError> {
    let Some(resume) = resume else {
        return Ok((adapter.spawn(request).await?, None));
    };
    let can_resume = adapter
        .capabilities()
        .declares(yunta_core::Capability::ResumeSession);
    let resumed = match (resume.fresh_prompt, can_resume) {
        (Some(_), false) => None,
        _ => Some(adapter.resume(resume.session, request.clone()).await),
    };
    match resumed {
        Some(Ok(session)) => Ok((session, Some(resume.session))),
        Some(Err(error)) if resume.fresh_prompt.is_none() => Err(error.into()),
        _ => {
            let brief = resume.fresh_prompt.unwrap_or_default();
            let degraded = yunta_core::events::CapabilityDegradedPayload::new(
                yunta_core::Capability::ResumeSession,
                adapter.id().clone(),
                yunta_core::port::Policy::FreshSession,
            );
            emit_audit(
                audit,
                EventPayload::Session(SessionEvent::CapabilityDegraded(degraded)),
            )
            .await
            .map_err(DispatchError::Audit)?;
            let fresh = crate::run::session_plan::with_prompt(request, brief.to_string());
            Ok((adapter.spawn(fresh).await?, None))
        }
    }
}

/// Ends a session the read stopped short of its own end, by a cancel or a
/// budget: ordered termination first, then forceful. Fails when the kill does.
pub(super) async fn end_running(session: &mut dyn AgentSession) -> yunta_core::Result<()> {
    // Never leave anything running: ordered termination first,
    // then forceful — mock has nothing to distinguish them, but a
    // real adapter's session may still close cleanly on interrupt.
    // best-effort: a session that refuses the interrupt is ended by the kill below.
    let _ = session.interrupt().await;
    session.allow_exit(INTERRUPT_GRACE_PERIOD).await;
    session.kill().await
}
