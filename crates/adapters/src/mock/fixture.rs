//! The `mock` adapter's fixture format (T3.2) — a YAML script of events
//! plus filesystem effects, parsed once and replayed on `spawn()`.

use std::path::PathBuf;

use serde::Deserialize;
use yunta_core::Capabilities;

#[derive(Debug, Clone, Deserialize)]
pub struct MockFixture {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub steps: Vec<MockStep>,
    #[serde(default)]
    pub effects: Vec<MockEffect>,
    pub outcome: MockOutcome,
}

fn default_model() -> String {
    "mock-model".to_string()
}

/// One progress event the session emits before its outcome, with an
/// optional delay before it — the fixture's way of injecting latency.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MockStep {
    ToolUse {
        name: String,
        target_digest: String,
        #[serde(default)]
        after_ms: u64,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        #[serde(default)]
        cached_input_tokens: Option<u64>,
        #[serde(default)]
        after_ms: u64,
    },
    Note {
        text: String,
        #[serde(default)]
        after_ms: u64,
    },
}

impl MockStep {
    pub fn after_ms(&self) -> u64 {
        match self {
            MockStep::ToolUse { after_ms, .. }
            | MockStep::Usage { after_ms, .. }
            | MockStep::Note { after_ms, .. } => *after_ms,
        }
    }
}

/// A file the session writes under the request's `cwd`, simulating the
/// agent's own edits. `blocked: true` simulates a write that falls
/// outside the node's declared scope — see `MockFixture`'s doc on how
/// `edit_hooks` changes what happens to it.
#[derive(Debug, Clone, Deserialize)]
pub struct MockEffect {
    pub path: PathBuf,
    pub content: String,
    #[serde(default)]
    pub blocked: bool,
}

/// How the session's stream ends. `Crash` and `Hang` exist to exercise
/// the engine's side of O2 and of interrupt/kill (T3.3) — a mock
/// terminal `Completed`/`Failed` on its own can't test either.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MockOutcome {
    Completed {
        summary: String,
    },
    Failed {
        message: String,
        retryable: bool,
    },
    /// The stream ends with no terminal event at all — a real crash.
    Crash,
    /// The stream never produces another item until `interrupt`/`kill`
    /// is called, at which point it ends (still with no terminal event,
    /// same as a forced kill would leave it).
    Hang,
}
