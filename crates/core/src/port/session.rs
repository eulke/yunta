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

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::{
    AdapterError, AdapterId, AgentName, Capabilities, Capability, ModelName, Pid, Result, Secret,
    SessionId,
};
use async_trait::async_trait;
use futures::stream::BoxStream;
use thiserror::Error;

/// A node's declared write scope, passed through to an adapter with
/// `edit_hooks` so it can block edits outside it as they happen.
pub type Glob = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionProfile {
    ReadOnly,
    Edit,
    Full,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Budget {
    pub max_tokens: Option<u64>,
    pub max_turns: Option<u32>,
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
    /// Portable named-agent selection; only populated if
    /// `capabilities().custom_agents`.
    pub agent: Option<AgentName>,
    pub permissions: PermissionProfile,
    /// Secrets arrive here, already resolved from the manifest — never
    /// any other way — and stay wrapped until the child process is
    /// spawned.
    pub env: HashMap<String, Secret<String>>,
    /// Globs the node/task declares, if the adapter has `edit_hooks`. An
    /// adapter without that capability ignores this field rather than
    /// failing — the engine already degraded and warned.
    pub edit_constraints: Option<Vec<Glob>>,
    pub budget: Budget,
    /// Adapter-specific settings with no portable expression — model,
    /// agent and permissions are typed fields above precisely so this
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
    /// `agent:`/`edit_hooks`); only populated when it declared
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
    /// make the name unique. The engine creates the path; an adapter
    /// creates the directory when it has something to put there. `None`
    /// leaves an adapter that needs one to degrade explicitly.
    pub scratch_dir: Option<PathBuf>,
}

/// Where a session's per-run MCP server listens: a loopback URL plus
/// the single-use bearer token that scopes every call to `(run_id,
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
    /// The name a CLI's own configuration gives this server. Every
    /// adapter uses the one name: a CLI prefixes the tools it mounts
    /// with it, so this is also what an allow-rule names to admit all
    /// of them without any adapter knowing which tools the engine
    /// mounted.
    pub const SERVER_NAME: &'static str = "yunta";
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
/// the engine's own criteria run regardless. One field for now, widened
/// when a real adapter has more worth reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOutcome {
    pub summary: String,
}

/// A session's failure, kept intentionally minimal until a real adapter
/// shows what richer information is actually available to report.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct AgentError {
    pub message: String,
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
    },
    /// How many of the run tools this session actually holds, as its
    /// CLI reported them. An adapter emits it only when its CLI names
    /// the session's tool set: silence means the adapter cannot tell,
    /// never that the count is zero, so a reader degrades only on a
    /// count it was actually given.
    RunToolsMounted {
        count: usize,
    },
    ToolUse {
        name: String,
        target_digest: String,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
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
        retryable: bool,
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

    async fn spawn(&self, req: SessionRequest) -> Result<Box<dyn AgentSession>>;

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
    /// without either, which is a crash: the engine, not the adapter,
    /// synthesizes `Failed { retryable: true }` for that case.
    fn events(&mut self) -> BoxStream<'_, AgentEvent>;

    /// Ordered termination (Esc/SIGINT-equivalent) — the agent may still
    /// close cleanly.
    async fn interrupt(&mut self) -> Result<()>;

    /// Forceful termination of the whole session process tree — never
    /// leaves anything running.
    async fn kill(&mut self) -> Result<()>;

    /// The OS process-group id of the session's subprocess tree, when
    /// the adapter runs one — what the engine registers in
    /// `run.dir/scratch/engine.json` so a *separate* process (`yunta
    /// cancel` after a crash) can still exterminate the tree.
    /// `None` for sessions with no subprocess of their own (mock).
    fn pgid(&self) -> Option<Pid> {
        None
    }
}
