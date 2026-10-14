//! The port an engine calls an agent CLI through.
//!
//! The engine knows an adapter by this trait, the request it hands over
//! and the events it reads back — never by a binary name, a flag or a
//! path. Which is why the port lives here, in the crate every other one
//! depends on, rather than beside the adapters that implement it: the
//! compiler is what keeps a concrete CLI out of the engine.
//!
//! `SessionRequest` drops `context: ResolvedContext` — the engine
//! inlines or references context in the prompt. Everything else in the
//! Adapter Spec's `SessionRequest` is here, including fields no caller
//! populates (`env`, `budget`, `adapter_settings`): they are struct
//! fields, not machinery, so an adapter can read what a workflow
//! declares the day one does.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::confinement::{Confinement, EditSandbox};
use crate::events::SessionExit;
use crate::fence::{Coverage, Fence};
use crate::rail::{
    answer::{FormatAnswer, StopAnswer, WriteAnswer},
    RailCall, RailCommand, RailEndpoint, RailEvent,
};
use crate::{
    AdapterError, AdapterId, Capabilities, Capability, ModelName, Pid, Result, Secret, SessionId,
};
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The profile a session runs under. The log reads it as `read_only`,
/// `edit` or `full`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionProfile {
    ReadOnly,
    Edit,
    Full,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Budget {
    pub max_tokens: Option<u64>,
    pub timeout: Option<Duration>,
}

/// A session request the engine hands to an adapter to open.
#[derive(Debug, Clone)]
pub struct SessionRequest {
    /// Already rendered by the engine — templates resolved.
    pub prompt: String,
    /// The run's worktree.
    pub cwd: PathBuf,
    pub model: Option<ModelName>,
    /// The agent this session works as, resolved by the run: instructions
    /// the adapter adds to its CLI's own. Only populated if
    /// `capabilities().custom_agents`.
    pub agent: Option<crate::agent::AgentDefinition>,
    /// How long the model reasons, the node's word over its runner's;
    /// absent leaves it to the CLI. Every adapter hands it over, a resumed
    /// session included.
    pub effort: Option<crate::Effort>,
    /// What this session may write and reach. The engine resolves it once,
    /// through `Confinement::of`; every adapter maps it exactly or refuses
    /// it with `Unbuildable`.
    pub confinement: Confinement,
    /// Whether the engine formats what this session writes, the moment it
    /// writes it: the adapter installs the hook that tells it only then.
    pub formats: bool,
    /// Whose configuration of the CLI the session loads beside what the
    /// run mounts: the person's own, or none but the run's.
    pub cli_config: crate::CliConfig,
    /// The environment, already resolved from the manifest — never any
    /// other way — and its secrets stay wrapped until the child process
    /// is spawned.
    pub env: SessionEnv,
    /// The scope the session's file-tool writes are judged against; what
    /// it may write beyond that is `confinement`. Always present. An
    /// adapter builds as much of it as its CLI can
    /// (`capabilities().fence`) and reports how much that was; an
    /// adapter that can build none of it ignores the field, and the
    /// engine already recorded the degradation.
    pub fence: Fence,
    /// The command a CLI's hooks run to ask the engine — before a write,
    /// first of all. `None` only in a harness with no binary to run; an
    /// adapter whose fence needs it and does not have it fails the
    /// session rather than opening one that writes freely.
    pub rail_command: Option<RailCommand>,
    /// Where those hooks reach the engine: the rail route of this
    /// session's own listener, with the same per-session token as its
    /// tools. `None` when the session has no listener.
    pub rail: Option<RailEndpoint>,
    pub budget: Budget,
    /// Adapter-specific settings with no portable expression — model,
    /// agent and confinement are typed fields above precisely so this
    /// stays for what genuinely has nowhere else to go.
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    /// Skill directories to expose to the agent — absolute paths the
    /// engine already resolved; the adapter mounts them by its native
    /// mechanism, and only when it declared `capabilities().skills`
    /// (the engine never populates this otherwise, since a capability
    /// must never claim more than is actually built).
    pub skills: Vec<std::path::PathBuf>,
    /// The per-run MCP endpoint for THIS session: a loopback HTTP
    /// listener the engine started just before this spawn, dead when
    /// the session ends — a resume always carries fresh credentials,
    /// never a reused pair. The adapter translates it to its CLI's
    /// native external-MCP mechanism (same pattern as
    /// `agent:`/`fence`); only populated when it declared
    /// `capabilities().run_tools` (same rule: never claim more than is
    /// actually built).
    pub run_tools_endpoint: Option<RunToolsEndpoint>,
    /// The one directory outside `cwd` this session may write, and where
    /// the files this node declares belong. It is never inside `cwd`:
    /// the worktree is the work, the run directory is the record. It
    /// belongs to this node alone, so a file written here is never a
    /// file another node produced. An adapter whose CLI confines writes
    /// to the working directory has to widen it to this path, or a node
    /// that declares a file can never produce one. `None` whenever the
    /// node has no file of its own to write — every document the engine
    /// itself writes from what the session hands over — and a session
    /// then reaches nothing outside its worktree.
    pub artifact_dir: Option<PathBuf>,
    /// This session's own scratch directory, for scaffolding it needs on
    /// disk — an MCP config file, say. It sits outside `cwd` because the
    /// worktree's diff is what the engine's scope check reads, and a
    /// file the adapter dropped there would read as the agent's work.
    ///
    /// It belongs to this session alone: sessions of one run that can be
    /// alive at the same moment each get their own, so an adapter may
    /// name a file inside it for what the file is rather than having to
    /// make the name unique. The engine resolves the path for every
    /// session; an adapter creates the directory when it has something
    /// to put there.
    pub scratch_dir: PathBuf,
}

/// A session's environment, in two maps by what a log may carry: `plain`
/// for shared directories and engine settings, `secret` for declared
/// secrets and session tokens, which are never logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionEnv {
    /// Shared directories and engine settings: not secrets.
    pub plain: BTreeMap<String, String>,
    /// Declared secrets and session tokens: never logged.
    pub secret: BTreeMap<String, Secret<String>>,
}

/// Where a session's per-run MCP server listens: a loopback URL plus
/// the per-session bearer token that scopes every call to `(run_id,
/// node_id, attempt)` by construction — no tool ever takes a run id as
/// a caller argument. The token is secret material: it must never
/// reach the event log — the engine's own audit events carry the URL
/// at most, never this pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunToolsEndpoint {
    pub url: String,
    pub token: Secret<String>,
}

impl RunToolsEndpoint {
    /// The file a session's adapter writes its tool endpoint to; it carries
    /// the session's bearer.
    pub const CONFIG_FILE: &'static str = "mcp.json";

    /// The name a CLI's own configuration gives this server. Every
    /// adapter uses the one name: a CLI prefixes the tools it mounts
    /// with it, so this is also what an allow-rule names to admit all
    /// of them without any adapter knowing which tools the engine
    /// mounted.
    ///
    /// It is not `yunta`, which is what a person registering the
    /// control plane in their CLI's own configuration calls it. A CLI
    /// merges both entries into one table by key, so two servers under
    /// one name is one server configured twice — and the second write
    /// contradicts the first, because the control plane is a command
    /// and this is a URL.
    pub const SERVER_NAME: &'static str = "yunta-run";

    /// How long one call to these tools may take before the CLI gives up
    /// on it: as long as the session itself may run, or an hour for a
    /// session with no timeout. A call can run a task's criteria,
    /// which take as long as the build behind them, and a CLI's own
    /// default would cut the answer off before it arrives.
    pub fn call_timeout(budget: &Budget) -> Duration {
        budget
            .timeout
            .unwrap_or(Duration::from_secs(crate::rail::window::UNSET_WINDOW_SECS))
    }
}

/// Health check result (`probe()` — binary present, version compatible,
/// auth valid).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeReport {
    /// The adapter can open sessions; `version` is what its CLI
    /// reported, when it reports one.
    Healthy { version: Option<String> },
    /// The adapter cannot open sessions, and why.
    Unhealthy { diagnostic: String },
}

/// What the agent itself reported it did — telemetry, never a verdict;
/// the engine's own criteria run regardless. It holds one field, the
/// summary; a real adapter that reports more worth keeping widens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOutcome {
    pub summary: String,
}

/// A session's failure: what went wrong, and whatever the adapter has
/// under it.
///
/// The cause is kept rather than flattened into the message, so a caller
/// following the chain reaches what the adapter actually caught — a
/// broken line, an I/O error, a CLI that answered something the protocol
/// does not allow — instead of a sentence somebody assembled about it.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct AgentError {
    pub message: String,
    #[source]
    pub cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl AgentError {
    /// A failure the adapter states in one sentence, with nothing under
    /// it.
    pub fn message(message: impl Into<String>) -> Self {
        AgentError {
            message: message.into(),
            cause: None,
        }
    }

    /// The same, keeping what the adapter caught.
    pub fn caused_by(
        message: impl Into<String>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        AgentError {
            message: message.into(),
            cause: Some(Box::new(cause)),
        }
    }
}

/// Two failures are the same failure when they say the same thing: the
/// cause is what a reader follows, never what a test compares.
impl PartialEq for AgentError {
    fn eq(&self, other: &Self) -> bool {
        self.message == other.message
    }
}

impl Eq for AgentError {}

impl Clone for AgentError {
    /// The message travels; the cause does not, because a boxed error is
    /// not clonable and the sentence is what every caller reads.
    fn clone(&self) -> Self {
        AgentError::message(self.message.clone())
    }
}

/// Events a session's stream carries.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// Mandatory first event of every session. `model` is the one the
    /// CLI reported, absent when it reports none — never the one the
    /// request asked for.
    SessionOpened {
        session_id: SessionId,
        model: Option<ModelName>,
        /// How much of the session the adapter's fence actually covers,
        /// derived from what it built. `None` when it built nothing.
        fence: Option<Coverage>,
    },
    /// The CLI rejected a call to a known tool of this session's
    /// `yunta-run` server. No arguments or CLI error text cross this
    /// boundary.
    RunToolRejected {
        tool: crate::RunTool,
    },
    ToolUse {
        name: String,
        /// What the call acted on. An adapter that can name a path says
        /// so; one reading the session's own text hands it over opaque,
        /// so what it typed never reaches the log.
        target: crate::events::ToolTarget,
    },
    /// What the CLI reported it spent. Every count is optional and
    /// absent means absent: a CLI that says nothing about a count has
    /// not said zero, and a run that recorded zero would report a
    /// session that cost nothing.
    Usage {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cached_input_tokens: Option<u64>,
    },
    Note {
        text: String,
    },
    Completed {
        result: AgentOutcome,
    },
    Failed {
        error: AgentError,
        cause: crate::events::FailureCause,
    },
}

/// Converts a session request into a stream of typed events — nothing
/// more. `id()`/`capabilities()` are sync: capabilities are fixed at
/// construction and never require I/O to report.
#[async_trait]
pub trait Adapter: Send + Sync {
    fn id(&self) -> &'static AdapterId;

    fn capabilities(&self) -> Capabilities;

    async fn probe(&self) -> Result<ProbeReport>;

    /// The worktree-relative paths a session opened for `req` writes
    /// for the adapter's own mechanics — a mount, a settings file —
    /// never the agent's work. The engine's scope check leaves exactly
    /// these out. Default: nothing.
    fn staged_paths(&self, _req: &SessionRequest) -> Vec<PathBuf> {
        Vec::new()
    }

    /// The trees of a checkout this adapter's CLI loads configuration from
    /// that runs code or mounts tools — its settings with their hooks, its
    /// MCP servers. No session of any run writes under them, whichever
    /// adapter the run drives: a pooled checkout outlives the session that
    /// wrote one, and the next session to open there would run what it says
    /// outside its sandbox. Every run protects the trees in
    /// [`crate::protected::AGENT_CONFIG`], which holds every adapter's.
    /// Default: none.
    fn own_config(&self) -> Vec<crate::ScopeGlob> {
        Vec::new()
    }

    /// The `sandbox` setting the adapter's CLI applies to `edit` sessions.
    /// The engine hands it to `Confinement::of` as one of its inputs, so the
    /// setting's effect is recorded with the session. Default: the
    /// workspace-write sandbox.
    fn edit_sandbox(&self) -> EditSandbox {
        EditSandbox::default()
    }

    /// How this adapter translates between its CLI's hooks and the
    /// engine's rail. `Some` exactly when `capabilities().fence` is
    /// [`crate::FenceLevel::ToolCalls`]: a sandbox needs no codec, and
    /// nothing to build needs none either.
    fn rail_codec(&self) -> Option<&dyn RailCodec> {
        None
    }

    async fn spawn(&self, req: SessionRequest) -> Result<Box<dyn AgentSession>>;

    /// The variables this adapter's CLI reaches its service by — a
    /// proxy, a base URL — which the shell captures for [`Self::reachable`].
    /// Default: none.
    fn service_vars(&self) -> &'static [&'static str] {
        &[]
    }

    /// Whether the service a session opened for `req` talks to answers
    /// from this machine now — `None` when the adapter cannot tell where
    /// that service is. `shell` is what the shell had for
    /// [`Self::service_vars`], which the session's CLI inherits. What a
    /// session cut off from its service waits on before it picks its work
    /// back up. Default: cannot tell.
    async fn reachable(
        &self,
        _req: &SessionRequest,
        _shell: &std::collections::BTreeMap<String, String>,
    ) -> Option<bool> {
        None
    }

    /// Default: unsupported. Only called if `capabilities().resume_session`.
    async fn resume(
        &self,
        _session: &SessionId,
        _req: SessionRequest,
    ) -> Result<Box<dyn AgentSession>> {
        Err(AdapterError::Unsupported {
            adapter: self.id().clone(),
            what: Capability::ResumeSession,
        })
    }
}

#[async_trait]
pub trait AgentSession: Send {
    /// Terminates with exactly one `Completed` or `Failed` — or ends
    /// without either, which is a session that died. The adapter never
    /// invents a terminal for that case: the engine asks [`Self::exit`]
    /// how the process went and records the death with that answer.
    fn events(&mut self) -> BoxStream<'_, AgentEvent>;

    /// Ordered termination (Esc/SIGINT-equivalent) — the agent may still
    /// close cleanly.
    async fn interrupt(&mut self) -> Result<()>;

    /// Forceful termination of the whole session process tree — never
    /// leaves anything running. An error means the engine cannot tell the
    /// session stopped, and its process group may still run.
    async fn kill(&mut self) -> Result<()>;

    /// Gives the session's process up to `grace` to exit on its own after
    /// an interrupt, answering the moment it does. A session with no
    /// process of its own has nothing to wait for and answers at once.
    async fn allow_exit(&mut self, _grace: Duration) {}

    /// The OS process-group id of the session's subprocess tree, when
    /// the adapter runs one — what the engine registers in
    /// `run.dir/scratch/engine.json` so a *separate* process (`yunta
    /// cancel` after a crash) can still exterminate the tree.
    /// `None` for sessions with no subprocess of their own (mock).
    fn pgid(&self) -> Option<Pid> {
        None
    }

    /// How the process ended, asked only of a session whose stream ended
    /// without a terminal event.
    ///
    /// The session is over by the time it is asked, so the group dies
    /// first and the status is collected after: the wait is bounded by
    /// construction and nothing outlives the run. A session with no
    /// process of its own answers `None`.
    async fn exit(&mut self) -> Result<Option<SessionExit>> {
        Ok(None)
    }

    /// Takes back what the launch set up for this session, once the
    /// session has ended. Default: nothing was set up, so nothing is taken
    /// back.
    async fn release(&mut self) -> Result<()> {
        Ok(())
    }
}

/// What one CLI's hooks send and expect back. The adapter writes only
/// this: what the engine answers is the engine's, the same for every
/// adapter.
pub trait RailCodec: Send + Sync {
    /// The call, read out of what the CLI sent on stdin for `event`. A
    /// call that does not read, or a write that names no path, is an
    /// error: there is no call to skip.
    fn decode(&self, event: RailEvent, stdin: &[u8]) -> std::result::Result<RailCall, CodecError>;

    /// The engine's answer to a write, in the shape this CLI reads.
    fn before_write(&self, answer: &WriteAnswer) -> HookReply;

    /// The engine's answer after a write, in the shape this CLI reads.
    fn after_write(&self, answer: &FormatAnswer) -> HookReply;

    /// The engine's answer when the session would stop, in the shape this CLI reads.
    fn before_stop(&self, answer: &StopAnswer) -> HookReply;
}

/// What the hook process leaves behind: a CLI reads the exit code, the
/// streams, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookReply {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit: i32,
}

/// A hook call this adapter's codec cannot read.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("the hook call is not JSON")]
    Json(#[from] serde_json::Error),
    #[error("the hook call has no `{0}`")]
    MissingField(&'static str),
}
