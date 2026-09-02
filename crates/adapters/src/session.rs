//! The `Adapter`/`AgentSession` traits.
//!
//! `SessionRequest` drops `context: ResolvedContext` — the engine
//! inlines/references context in the prompt — and
//! `run_tools_endpoint: Option<Endpoint>` isn't wired for MCP yet.
//! `skills` is present. Everything else in
//! the spec's `SessionRequest` is here, even where nothing populates a
//! field yet (`env`, `budget`, `adapter_settings`) — those are simple
//! struct fields, not machinery to build, so there is no reason to defer
//! them the way whole subsystems get deferred.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use thiserror::Error;
use yunta_core::{
    AdapterId, AgentName, Capabilities, ModelName, Pid, Result, Secret, SessionId, YuntaError,
};

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

/// Reads an adapter's `adapter_settings` map into its typed settings:
/// every key must be one of `known`, and the typed struct reads the
/// values. The one place a setting name is checked, so an unknown key
/// is always the same error whichever adapter it reaches.
pub fn typed_settings<T: serde::de::DeserializeOwned>(
    adapter: &'static AdapterId,
    raw: Option<&serde_json::Map<String, serde_json::Value>>,
    known: &'static [&'static str],
) -> Result<T> {
    let raw = raw.cloned().unwrap_or_default();
    if let Some(key) = raw.keys().find(|key| !known.contains(&key.as_str())) {
        return Err(YuntaError::UnknownSetting {
            adapter: adapter.clone(),
            key: key.clone(),
            known: known.to_vec(),
        });
    }
    serde_json::from_value(serde_json::Value::Object(raw)).map_err(|e| YuntaError::Adapter {
        adapter: adapter.clone(),
        message: format!("`adapter_settings`: {e}"),
    })
}

/// Hands `prompt` to a just-spawned CLI on its stdin and closes the
/// pipe, so the CLI sees end-of-input and the prompt never appears in
/// an argument list. Called once the CLI's output is being read, so a
/// CLI that talks before it listens cannot deadlock the exchange. A CLI
/// that exits before reading closes the pipe on its side; that is the
/// session's own ending, reported by its stream, never a failure of the
/// write.
pub async fn write_prompt(
    mut stdin: tokio::process::ChildStdin,
    prompt: &str,
    adapter: &'static AdapterId,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let io_error = |action: &str, source: std::io::Error| YuntaError::AdapterIo {
        adapter: adapter.clone(),
        action: action.to_string(),
        source,
    };
    match stdin.write_all(prompt.as_bytes()).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Ok(()),
        Err(e) => return Err(io_error("write the prompt to the subprocess's stdin", e)),
    }
    match stdin.shutdown().await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(io_error("close the subprocess's stdin", e)),
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
/// the engine's own criteria run regardless.
/// `[inferido]`: the spec names this type without detailing its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOutcome {
    pub summary: String,
}

/// `[inferido]`: the spec names `AgentError` without detailing its
/// fields; kept intentionally minimal until a real adapter shows what
/// richer information is actually available to report.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct AgentError {
    pub message: String,
}

/// Events a session's stream carries.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// Mandatory first event of every session.
    SessionOpened {
        session_id: SessionId,
        model: ModelName,
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

    async fn spawn(&self, req: SessionRequest) -> Result<Box<dyn AgentSession>>;

    /// Default: unsupported. Only called if `capabilities().resume_session`.
    async fn resume(
        &self,
        _session: &SessionId,
        _req: SessionRequest,
    ) -> Result<Box<dyn AgentSession>> {
        Err(YuntaError::Unsupported {
            adapter: self.id().clone(),
            what: "resume_session",
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
