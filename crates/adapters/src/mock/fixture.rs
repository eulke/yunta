//! The `mock` adapter's fixture format — a YAML script of events
//! plus filesystem effects, parsed once and replayed on `spawn()`.
//!
//! A fixture scripts a whole run, not a single session: a workflow test
//! declares one fixture for everything its run spawns. Two forms parse:
//!
//! - **multi-session**: `sessions:` lists one script per spawn, consumed
//!   in spawn order — deterministic because execution is sequential;
//! - **single-session**: the script's fields at the top level — sugar
//!   for a one-entry `sessions:`.
//!
//! Spawning past the end of the script is an explicit adapter error,
//! never a silent replay of the last session. A person writes fixtures,
//! so every shape here refuses a key it does not know.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use yunta_core::yaml::{self, Value};
use yunta_core::{Capabilities, ModelName};

/// A parsed fixture: adapter-level capabilities plus one script per
/// expected `spawn()`, in order.
#[derive(Debug, Clone)]
pub struct MockFixture {
    pub capabilities: Capabilities,
    pub sessions: Vec<SessionScript>,
}

/// The run directories a fixture names. They are the caller's to
/// compute — a fixture is parsed before any run exists in some callers
/// and beside a live one in others — and this is what they are called
/// inside the YAML.
#[derive(Debug, Clone, Copy)]
pub struct RunPaths<'a> {
    /// `{{run.dir}}` — the run's own directory.
    pub run_dir: &'a Path,
    /// `{{worktree}}` — the checkout the session works in.
    pub worktree: &'a Path,
    /// `{{staging}}` — the root under which each node writes the files
    /// it declares, one directory per node id: a session scripted to
    /// produce an artifact of node `grill` writes
    /// `{{staging}}/grill/<name>`, which is exactly the directory that
    /// session is granted.
    pub staging: &'a Path,
}

/// Why a fixture did not become a script.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// The YAML names a variable these paths do not define, or leaves a
    /// `{{` unclosed.
    #[error("the fixture's run paths could not be resolved")]
    Paths(#[from] yunta_core::template::TemplateError),
    /// The rendered YAML is not a fixture. The document's own
    /// diagnostic is the message: it already names the key, the path
    /// and what it expected, which is what the person who wrote the
    /// fixture needs.
    #[error(transparent)]
    Shape(#[from] yunta_core::yaml::YamlError),
}

impl MockFixture {
    /// Reads one fixture, resolving the run directories it names against
    /// `paths` before the YAML is parsed.
    ///
    /// The one way a scripted session comes to exist. Rendering here
    /// rather than in each caller is what makes a fixture mean the same
    /// thing from the `yunta test` harness, from `yunta run --adapter
    /// mock --fixture` and from the run bench — the paths differ, the
    /// document does not.
    pub fn parse(yaml: &str, paths: &RunPaths<'_>) -> Result<Self, FixtureError> {
        Self::render(
            yaml,
            BTreeMap::from([
                ("run.dir".to_string(), paths.run_dir.display().to_string()),
                ("worktree".to_string(), paths.worktree.display().to_string()),
                ("staging".to_string(), paths.staging.display().to_string()),
            ]),
        )
    }

    /// One fixture for a caller that has no run: the same door with no
    /// directory defined, so a fixture that names one is refused here
    /// instead of scripting the literal `{{staging}}` as a path.
    pub fn parse_without_a_run(yaml: &str) -> Result<Self, FixtureError> {
        Self::render(yaml, BTreeMap::new())
    }

    fn render(yaml: &str, vars: BTreeMap<String, String>) -> Result<Self, FixtureError> {
        let rendered = yunta_core::template::render_template(yaml, &vars)?;
        Ok(yunta_core::yaml::parse(&rendered)?)
    }
}

impl<'de> Deserialize<'de> for MockFixture {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        // The two forms are told apart structurally — by the presence of
        // a `sessions` key — never by guessing from what parses.
        let value = Value::deserialize(deserializer)?;
        let has_sessions = value
            .as_mapping()
            .is_some_and(|m| m.contains_key(Value::from("sessions")));

        if has_sessions {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Multi {
                #[serde(default)]
                capabilities: FixtureCapabilities,
                sessions: Vec<SessionScript>,
            }
            let multi: Multi = yaml::from_value(value).map_err(D::Error::custom)?;
            Ok(MockFixture {
                capabilities: multi.capabilities.into(),
                sessions: multi.sessions,
            })
        } else {
            // The single-session form is a script with `capabilities`
            // alongside it: that key is taken out first, so the script
            // itself is read as strictly as in the `sessions:` form.
            let Value::Mapping(mut mapping) = value else {
                return Err(D::Error::custom(
                    "a fixture is a mapping: `sessions:` with one script per session, or \
                     one session's own fields",
                ));
            };
            let capabilities: FixtureCapabilities = match mapping.remove("capabilities") {
                Some(value) => yaml::from_value(value).map_err(D::Error::custom)?,
                None => FixtureCapabilities::default(),
            };
            let script: SessionScript =
                yaml::from_value(Value::Mapping(mapping)).map_err(D::Error::custom)?;
            Ok(MockFixture {
                capabilities: capabilities.into(),
                sessions: vec![script],
            })
        }
    }
}

/// The capabilities a fixture declares — [`Capabilities`] flag for
/// flag. The event log embeds `Capabilities` and reads what a later
/// writer adds; a fixture is authored, so this twin refuses a flag it
/// does not know.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FixtureCapabilities {
    pub resume_session: bool,
    pub edit_hooks: bool,
    pub permission_profiles: bool,
    pub custom_agents: bool,
    pub usage_reporting: bool,
    pub skills: bool,
    pub run_tools: bool,
    pub network_isolation: bool,
}

impl From<FixtureCapabilities> for Capabilities {
    fn from(fixture: FixtureCapabilities) -> Self {
        Capabilities {
            resume_session: fixture.resume_session,
            edit_hooks: fixture.edit_hooks,
            permission_profiles: fixture.permission_profiles,
            custom_agents: fixture.custom_agents,
            usage_reporting: fixture.usage_reporting,
            skills: fixture.skills,
            run_tools: fixture.run_tools,
            network_isolation: fixture.network_isolation,
        }
    }
}

/// What one spawned session does: the model it announces, the events
/// it emits, the files it writes, and how its stream ends. The agent
/// is the request's, never the script's: the mock records what the
/// engine asked for and announces nothing of its own.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionScript {
    #[serde(default = "default_model")]
    pub model: ModelName,
    #[serde(default)]
    pub steps: Vec<MockStep>,
    #[serde(default)]
    pub effects: Vec<MockEffect>,
    pub outcome: MockOutcome,
    /// Selects this script by a substring of the spawning request's own
    /// prompt, instead of by call order: concurrent task dispatch means
    /// several `spawn()` calls race, so pure declaration-order
    /// consumption can no longer promise which request gets which
    /// script. Absent — the vast majority of fixtures, unchanged — keeps
    /// today's exact behavior: consumed strictly in declaration order,
    /// among the other unmatched scripts.
    #[serde(default)]
    pub match_prompt_contains: Option<String>,
}

fn default_model() -> ModelName {
    ModelName::from_static("mock-model")
}

/// One progress event the session emits before its outcome, with an
/// optional delay before it — the fixture's way of injecting latency.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
    /// What a CLI reports about the per-run tools the session holds:
    /// how many of them it actually mounted. A fixture scripts it to
    /// put a session in the state a client that could not read the
    /// server's tool list leaves behind — the server mounted, the
    /// session holding nothing from it.
    RunToolsMounted {
        count: usize,
        #[serde(default)]
        after_ms: u64,
    },
    /// Performs a REAL MCP `tools/call` against the session's own
    /// `run_tools_endpoint` — the mock as a genuine client of the
    /// engine's per-run listener, over the wire. A fixture using this
    /// on a session the engine gave no endpoint is an authoring error
    /// and fails the session loudly, never silently skips.
    RunTool {
        tool: String,
        #[serde(default)]
        arguments: serde_json::Map<String, serde_json::Value>,
        #[serde(default)]
        expect: ToolExpectation,
        #[serde(default)]
        after_ms: u64,
    },
}

/// What a scripted tool call expects the engine to answer.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExpectation {
    /// The call succeeds. A tool error fails the session, which is what
    /// a fixture that scripted a working call means by scripting it.
    #[default]
    Accepted,
    /// The engine refuses the call and the session goes on — the shape
    /// every refusal has: a diagnostic the session can act on, not the
    /// end of it. A success fails the session instead, so a fixture
    /// cannot claim a refusal it did not get.
    Refused,
}

impl MockStep {
    pub fn after_ms(&self) -> u64 {
        match self {
            MockStep::ToolUse { after_ms, .. }
            | MockStep::Usage { after_ms, .. }
            | MockStep::Note { after_ms, .. }
            | MockStep::RunToolsMounted { after_ms, .. }
            | MockStep::RunTool { after_ms, .. } => *after_ms,
        }
    }
}

/// A file the session writes under the request's `cwd`, simulating the
/// agent's own edits. Whether it lands is the request's business: with
/// `edit_hooks` declared, a path outside the request's
/// `edit_constraints` is blocked before it is written, exactly as a
/// hook-capable CLI would; without the capability every effect lands
/// and the engine's post-check scope diff is what catches it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MockEffect {
    pub path: PathBuf,
    pub content: String,
}

/// How the session's stream ends. `Crash` and `Hang` exist to exercise
/// the engine's side of detecting a session that ends with no terminal
/// event, and of interrupt/kill — a mock terminal `Completed`/`Failed`
/// on its own can't test either.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
    /// The stream never produces another item on its own; `kill` ends
    /// it, and `interrupt` ends it or not as `on_interrupt` says (still
    /// with no terminal event, as a forced kill leaves a real session).
    Hang {
        #[serde(default)]
        on_interrupt: OnInterrupt,
    },
}

/// What a hung session does with an ordered stop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnInterrupt {
    /// Ends, as a CLI that honors SIGINT does.
    #[default]
    End,
    /// Keeps hanging; only `kill` ends it — the case the engine's
    /// interrupt-then-kill escalation exists for.
    Ignore,
}
