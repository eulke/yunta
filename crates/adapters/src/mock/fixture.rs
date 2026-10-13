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
//! Both forms name the CLI the fixture stands for with `dialect:`.
//! Spawning past the end of the script is an explicit adapter error,
//! never a silent replay of the last session. A person writes fixtures,
//! so every shape here refuses a key it does not know.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use yunta_core::port::FailureCause;
use yunta_core::template::TemplateVar;
use yunta_core::yaml::{self, Value};
use yunta_core::ModelName;

use super::check;
use crate::dialect::DialectName;

/// A parsed fixture: the CLI it stands for and one script per expected
/// `spawn()`, in order.
#[derive(Debug, Clone)]
pub struct MockFixture {
    /// The CLI this fixture stands for. Its launch, its fence, its hooks
    /// and how it names tools all come from it, so a fixture scripts what
    /// that CLI does and declares none of its own.
    dialect: DialectName,
    pub sessions: Vec<SessionScript>,
    /// How many times the service the mock stands for answers that it
    /// cannot be reached before it answers that it can — what a session
    /// cut off from it waits on.
    pub service_down_for: u32,
    /// Whether a session lists its run tools before its first turn, as
    /// every real CLI does. `false` stands for one whose tools never
    /// reached it.
    pub lists_tools: bool,
}

/// What a fixture that says nothing about listing does: list, as every
/// real CLI does.
fn lists() -> bool {
    true
}

/// The run directories a fixture names. They are the caller's to
/// compute — a fixture is parsed before any run exists in some callers
/// and beside a live one in others — and this is what they are called
/// inside the YAML.
#[derive(Debug, Clone, Copy)]
pub struct RunPaths<'a> {
    /// `{{run.dir}}` — the run's own directory.
    pub run_dir: &'a Path,
    /// `{{run.worktree}}` — the checkout the session works in.
    pub worktree: &'a Path,
    /// `{{run.staging}}` — the root under which each node writes the files
    /// it declares, one directory per node id: a session scripted to
    /// produce an artifact of node `grill` writes
    /// `{{run.staging}}/grill/<name>`, which is exactly the directory that
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
    /// A usage step a codex session cannot print where the script puts it.
    #[error(
        "a codex session reports its usage on the line that completes its turn, so a usage \
         step must be the last step of a session that completes"
    )]
    UsageNotLast,
    /// A declared cause the outcome's message does not classify to.
    #[error("the outcome declares the cause {declared}, but its message classifies as {actual}")]
    Cause {
        declared: &'static str,
        actual: &'static str,
    },
    /// A `tool_use` names a run tool by its protocol name, which the CLI
    /// does not call it by.
    #[error(
        "a tool_use names the run tool `{tool}` by its protocol name: this CLI calls it \
         `{called}`, so write that"
    )]
    ProtocolRunTool { tool: &'static str, called: String },
}

impl MockFixture {
    /// The CLI this fixture stands for.
    pub(crate) fn dialect(&self) -> DialectName {
        self.dialect
    }

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
                (TemplateVar::RunDir, paths.run_dir.display().to_string()),
                (TemplateVar::Worktree, paths.worktree.display().to_string()),
                (TemplateVar::Staging, paths.staging.display().to_string()),
            ]),
        )
    }

    /// One fixture for a caller that has no run: the same door with no
    /// directory defined, so a fixture that names one is refused here
    /// instead of scripting the literal `{{run.staging}}` as a path.
    pub fn parse_without_a_run(yaml: &str) -> Result<Self, FixtureError> {
        Self::render(yaml, BTreeMap::new())
    }

    fn render(yaml: &str, vars: BTreeMap<TemplateVar, String>) -> Result<Self, FixtureError> {
        let rendered = yunta_core::template::render_template(yaml, &vars)?;
        let fixture: Self = yunta_core::yaml::parse(&rendered)?;
        fixture.check()?;
        Ok(fixture)
    }

    /// Refuses what a fixture states that its CLI would not do: a declared
    /// cause its message does not classify to, and a run tool named by its
    /// protocol name where the CLI prefixes the names of its tools.
    fn check(&self) -> Result<(), FixtureError> {
        let naming = self.dialect.capabilities().tool_naming;
        for session in &self.sessions {
            session.outcome.check()?;
            check::names(&session.steps, naming)?;
            if let Some(stop) = &session.before_stop {
                check::names(&stop.steps, naming)?;
            }
            check::usage(self.dialect, session)?;
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for MockFixture {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // The two forms are told apart structurally — by the presence of
        // a `sessions` key — never by guessing from what parses.
        let value = Value::deserialize(deserializer)?;
        let has_sessions = value
            .as_mapping()
            .is_some_and(|m| m.contains_key("sessions"));
        if has_sessions {
            multi_session(value)
        } else {
            single_session(value)
        }
    }
}

/// The `sessions:` form: the fixture's own keys, and one script per session.
fn multi_session<E: serde::de::Error>(value: Value) -> Result<MockFixture, E> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Multi {
        dialect: DialectName,
        sessions: Vec<SessionScript>,
        #[serde(default)]
        service_down_for: u32,
        #[serde(default = "lists")]
        lists_tools: bool,
    }
    let multi: Multi = yaml::from_value(value).map_err(E::custom)?;
    Ok(MockFixture {
        dialect: multi.dialect,
        sessions: multi.sessions,
        service_down_for: multi.service_down_for,
        lists_tools: multi.lists_tools,
    })
}

/// The single-session form: the script's fields alongside the fixture's own
/// keys. Those keys are taken out first, so the script itself is read as
/// strictly as in the `sessions:` form.
fn single_session<E: serde::de::Error>(value: Value) -> Result<MockFixture, E> {
    let Value::Mapping(mut mapping) = value else {
        return Err(E::custom(
            "a fixture is a mapping: `sessions:` with one script per session, or one \
             session's own fields",
        ));
    };
    let dialect: DialectName = match mapping.remove("dialect") {
        Some(value) => yaml::from_value(value).map_err(E::custom)?,
        None => {
            return Err(E::custom(
                "a fixture names the CLI it stands for: `dialect: claude-code` or \
                 `dialect: codex`",
            ))
        }
    };
    let lists_tools: bool = match mapping.remove("lists_tools") {
        Some(value) => yaml::from_value(value).map_err(E::custom)?,
        None => true,
    };
    let script: SessionScript = yaml::from_value(Value::Mapping(mapping)).map_err(E::custom)?;
    Ok(MockFixture {
        dialect,
        sessions: vec![script],
        service_down_for: 0,
        lists_tools,
    })
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
    /// Whether the session asks the engine before it stops, as a CLI's
    /// stop hook does, and what it writes when it is sent back. Absent,
    /// it stops without asking.
    #[serde(default)]
    pub before_stop: Option<MockStop>,
}

/// A session that asks before it stops, and what it does the first time
/// it is sent back, before it asks again — nothing, for one that asks
/// again as it was.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MockStop {
    /// Files it writes, each asked about as its other writes are.
    #[serde(default)]
    pub writes: Vec<MockEffect>,
    /// Steps it plays, after those writes.
    #[serde(default)]
    pub steps: Vec<MockStep>,
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
        /// What the scripted call acts on. A fixture writes what a real
        /// session would hand over, so the mock produces the same
        /// opaque target every other adapter does.
        target: String,
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
    /// The lines of every kind the CLI prints that its reader does not read,
    /// as the CLI writes them, which the reader tolerates.
    UnknownLine {
        #[serde(default)]
        after_ms: u64,
    },
    /// A text line the CLI prints before it opens its session. The player
    /// prints it ahead of the open line, where the stream guard refuses it.
    BeforeOpen {
        text: String,
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
            | MockStep::UnknownLine { after_ms }
            | MockStep::BeforeOpen { after_ms, .. }
            | MockStep::RunTool { after_ms, .. } => *after_ms,
        }
    }
}

/// A file the session writes under the request's `cwd`, simulating the
/// agent's own edits. Whether it lands is the request's business: with a
/// fence declared, a path the request's fence refuses is never written,
/// exactly as a hook-capable CLI would; without one every effect lands
/// and the engine's post-check scope diff is what catches it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MockEffect {
    pub path: PathBuf,
    pub content: String,
    /// How the write is made. A write through the CLI's own tool call is
    /// asked of the engine's rail before it lands; a write through a shell
    /// command reaches no write hook, so the rail is never asked about it.
    #[serde(default)]
    pub via: Via,
}

/// How a scripted write is made: through the CLI's tool call, which its
/// write hook judges, or through a shell command, which no hook sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// Through the CLI's tool call, which the write hook judges first.
    #[default]
    Tool,
    /// Through a shell command, which lands without the write hook seeing it.
    Command,
}

/// The causes a fixture may state: the port's three, as a person writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclaredCause {
    Retryable,
    Final,
    Unreachable,
}

impl From<DeclaredCause> for FailureCause {
    fn from(declared: DeclaredCause) -> Self {
        match declared {
            DeclaredCause::Retryable => FailureCause::Retryable,
            DeclaredCause::Final => FailureCause::Final,
            DeclaredCause::Unreachable => FailureCause::Unreachable,
        }
    }
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
        /// The cause the message must classify to, when the fixture states
        /// one. A fixture whose message classifies otherwise is refused
        /// when it is read.
        #[serde(default)]
        cause: Option<DeclaredCause>,
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

impl MockOutcome {
    /// Refuses a declared cause that the message does not classify to.
    pub(crate) fn check(&self) -> Result<(), FixtureError> {
        let MockOutcome::Failed {
            message,
            cause: Some(declared),
        } = self
        else {
            return Ok(());
        };
        let declared = FailureCause::from(*declared);
        let actual = crate::failure::classify(message).cause();
        if declared == actual {
            return Ok(());
        }
        Err(FixtureError::Cause {
            declared: word(declared),
            actual: word(actual),
        })
    }
}

/// The word a fixture writes a cause as.
fn word(cause: FailureCause) -> &'static str {
    match cause {
        FailureCause::Retryable => "retryable",
        FailureCause::Final => "final",
        FailureCause::Unreachable => "unreachable",
    }
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
