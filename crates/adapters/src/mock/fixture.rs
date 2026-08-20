//! The `mock` adapter's fixture format (T3.2) — a YAML script of events
//! plus filesystem effects, parsed once and replayed on `spawn()`.
//!
//! A fixture scripts a whole run, not a single session (Contrato §14: a
//! workflow test declares one fixture for everything its run spawns).
//! Two forms parse:
//!
//! - **multi-session**: `sessions:` lists one script per spawn, consumed
//!   in spawn order — deterministic because M-0 execution is sequential;
//! - **single-session**: the script's fields at the top level — sugar
//!   for a one-entry `sessions:`.
//!
//! Spawning past the end of the script is an explicit adapter error,
//! never a silent replay of the last session.

use std::path::PathBuf;

use serde::Deserialize;
use yunta_core::Capabilities;

/// A parsed fixture: adapter-level capabilities plus one script per
/// expected `spawn()`, in order.
#[derive(Debug, Clone)]
pub struct MockFixture {
    pub capabilities: Capabilities,
    pub sessions: Vec<SessionScript>,
}

impl<'de> Deserialize<'de> for MockFixture {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        // The two forms are told apart structurally — by the presence of
        // a `sessions` key — never by guessing from what parses.
        let value = serde_yaml::Value::deserialize(deserializer)?;
        let has_sessions = value
            .as_mapping()
            .is_some_and(|m| m.contains_key(serde_yaml::Value::from("sessions")));

        if has_sessions {
            #[derive(Deserialize)]
            struct Multi {
                #[serde(default)]
                capabilities: Capabilities,
                sessions: Vec<SessionScript>,
            }
            let multi: Multi = serde_yaml::from_value(value).map_err(D::Error::custom)?;
            Ok(MockFixture {
                capabilities: multi.capabilities,
                sessions: multi.sessions,
            })
        } else {
            #[derive(Deserialize)]
            struct Single {
                #[serde(default)]
                capabilities: Capabilities,
                #[serde(flatten)]
                script: SessionScript,
            }
            let single: Single = serde_yaml::from_value(value).map_err(D::Error::custom)?;
            Ok(MockFixture {
                capabilities: single.capabilities,
                sessions: vec![single.script],
            })
        }
    }
}

/// What one spawned session does: its announced model/agent, the events
/// it emits, the files it writes, and how its stream ends.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionScript {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub steps: Vec<MockStep>,
    #[serde(default)]
    pub effects: Vec<MockEffect>,
    pub outcome: MockOutcome,
    /// Selects this script by a substring of the spawning request's own
    /// prompt, instead of by call order (T5.10: concurrent task dispatch
    /// means several `spawn()` calls race, so pure declaration-order
    /// consumption can no longer promise which request gets which
    /// script). Absent — the vast majority of fixtures, unchanged — keeps
    /// today's exact behavior: consumed strictly in declaration order,
    /// among the other unmatched scripts.
    #[serde(default)]
    pub match_prompt_contains: Option<String>,
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
    /// T8.2/A8: perform a REAL MCP `tools/call` against the session's
    /// own `run_tools_endpoint` — the mock as a genuine client of the
    /// engine's per-run listener, over the wire. A fixture using this
    /// on a session the engine gave no endpoint is an authoring error
    /// and fails the session loudly, never silently skips.
    RunTool {
        tool: String,
        #[serde(default)]
        arguments: serde_json::Map<String, serde_json::Value>,
        #[serde(default)]
        after_ms: u64,
    },
}

impl MockStep {
    pub fn after_ms(&self) -> u64 {
        match self {
            MockStep::ToolUse { after_ms, .. }
            | MockStep::Usage { after_ms, .. }
            | MockStep::Note { after_ms, .. }
            | MockStep::RunTool { after_ms, .. } => *after_ms,
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
