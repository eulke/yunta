//! The `Adapter`/`AgentSession` traits (Spec Adapter v0.2, T3.1).
//!
//! **M-0 cut**: `SessionRequest` drops `context: ResolvedContext` (the
//! engine inlines/references context in the prompt, T6.1) and
//! `run_tools_endpoint: Option<Endpoint>` (MCP is M8). `skills` came
//! back with DI-13. Everything else in
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
use yunta_core::{Capabilities, Result, SessionId, YuntaError};

/// A node's declared write scope, passed through to an adapter with
/// `edit_hooks` so it can block edits outside it as they happen (O5).
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

/// A session request the engine hands to an adapter to open (Spec
/// Adapter §2).
#[derive(Debug, Clone)]
pub struct SessionRequest {
    /// Already rendered by the engine — templates resolved.
    pub prompt: String,
    /// The run's worktree.
    pub cwd: PathBuf,
    pub model: Option<String>,
    /// Portable named-agent selection; only populated if
    /// `capabilities().custom_agents`.
    pub agent: Option<String>,
    pub permissions: PermissionProfile,
    /// Secrets arrive here, already resolved from the manifest — never
    /// any other way (I12).
    pub env: HashMap<String, String>,
    /// Globs the node/task declares, if the adapter has `edit_hooks`. An
    /// adapter without that capability ignores this field rather than
    /// failing (O5) — the engine already degraded and warned.
    pub edit_constraints: Option<Vec<Glob>>,
    pub budget: Budget,
    /// Adapter-specific settings with no portable expression — model,
    /// agent and permissions are typed fields above precisely so this
    /// stays for what genuinely has nowhere else to go.
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    /// Skill directories to expose to the agent (Spec Adapter v0.2,
    /// DI-13) — absolute paths the engine already resolved; the adapter
    /// mounts them by its native mechanism, and only when it declared
    /// `capabilities().skills` (the engine never populates this
    /// otherwise, A2/A6).
    pub skills: Vec<std::path::PathBuf>,
}

/// Health check result (`probe()` — binary present, version compatible,
/// auth valid).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReport {
    pub healthy: bool,
    pub version: Option<String>,
    pub diagnostic: Option<String>,
}

/// What the agent itself reported it did — telemetry, never a verdict
/// (Contrato §1; the engine's own criteria run regardless, §5.2).
/// `[inferido]`: the spec names this type without detailing its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOutcome {
    pub summary: String,
}

/// `[inferido]`: the spec names `AgentError` without detailing its
/// fields; kept intentionally minimal until a real adapter (T7.3) shows
/// what richer information is actually available to report.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct AgentError {
    pub message: String,
}

/// Events a session's stream carries (Spec Adapter §3).
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// Mandatory first event of every session (O1).
    SessionOpened {
        session_id: SessionId,
        model: String,
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
/// more (§1). `id()`/`capabilities()` are sync: capabilities are fixed at
/// construction (A2) and never require I/O to report.
#[async_trait]
pub trait Adapter: Send + Sync {
    fn id(&self) -> &'static str;

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
            adapter: self.id().to_string(),
            what: "resume_session",
        })
    }
}

#[async_trait]
pub trait AgentSession: Send {
    /// Terminates with exactly one `Completed` or `Failed` (A3) — or ends
    /// without either, which is a crash (O2): the engine, not the
    /// adapter, synthesizes `Failed { retryable: true }` for that case.
    fn events(&mut self) -> BoxStream<'_, AgentEvent>;

    /// Ordered termination (Esc/SIGINT-equivalent) — the agent may still
    /// close cleanly.
    async fn interrupt(&mut self) -> Result<()>;

    /// Forceful termination of the whole session process tree (A4) —
    /// never leaves anything running.
    async fn kill(&mut self) -> Result<()>;

    /// The OS process-group id of the session's subprocess tree, when
    /// the adapter runs one (DI-08) — what the engine registers in
    /// `run.dir/scratch/engine.json` so a *separate* process (`yunta
    /// cancel` after a crash) can still exterminate the tree (A4).
    /// `None` for sessions with no subprocess of their own (mock).
    fn pgid(&self) -> Option<u32> {
        None
    }
}
