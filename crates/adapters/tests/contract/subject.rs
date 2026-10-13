//! What the contract cases run against: the mock, and the two real adapters
//! driving their scripted stubs.
//!
//! A subject plays one `Script` and reports the events its adapter emitted,
//! the argv its CLI was launched with and the worktree it ran in. A case
//! reads a CLI's flags only through `read_back`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde_json::{json, Value};
use tokio::signal::unix::{signal, Signal, SignalKind};
use yunta_adapters::{ClaudeCodeAdapter, CodexAdapter, MockAdapter, MockFixture};
use yunta_core::port::{Adapter, AgentEvent, SessionRequest};
use yunta_core::rail::RailEvent;
use yunta_core::{AdapterSettings, Capabilities, FenceLevel, RunTool, ToolNaming};
use yunta_testkit_core::adapter::{drain, write_lines};
use yunta_testkit_core::stubs;

use super::wire;

/// The text a CLI prints for the turn it takes after the engine refuses its
/// stop. The harness writes it, and the stubs recognise a turn by it.
pub const STOP_TURN: &str = "a refused stop, and the work goes on: ";

/// The hook events a CLI's settings name, each with the engine's rail event.
const HOOK_EVENTS: [(&str, RailEvent); 3] = [
    ("PreToolUse", RailEvent::BeforeWrite),
    ("Stop", RailEvent::BeforeStop),
    ("PostToolUse", RailEvent::AfterWrite),
];

/// What one played session produced: the events its adapter emitted, the
/// argv its CLI was launched with, and the worktree it ran in.
#[derive(Debug)]
pub struct Played {
    pub events: Vec<AgentEvent>,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
}

/// One scripted session: the model its CLI reports, what it does, and how
/// its stream ends.
#[derive(Debug, Clone)]
pub struct Script {
    /// The model the CLI reports in its stream; `None` reports none.
    pub model: Option<String>,
    pub steps: Vec<Step>,
    pub outcome: Outcome,
}

/// How a scripted session's stream ends.
#[derive(Debug, Clone)]
pub enum Outcome {
    Completed(String),
    Failed(String),
    /// The stream ends with no terminal line at all.
    Crash,
}

/// One thing a scripted session does before its outcome.
#[derive(Debug, Clone)]
pub enum Step {
    Note(String),
    ToolUse {
        name: String,
        target: String,
    },
    /// A call to one of the run tools, named the way the CLI names it.
    RunToolCalled(RunTool),
    /// Tokens the session spent: `input` is all it read, and `cached` the part
    /// of that read from the cache, absent when the CLI reports none.
    Usage {
        input: u64,
        output: u64,
        cached: Option<u64>,
    },
    /// The engine refused the CLI's stop; the CLI takes one more turn on
    /// `reason`.
    StopRefused {
        reason: String,
    },
    /// The lines of every kind the CLI prints that its stream reader does not
    /// read, each as the CLI writes it, which a reader must tolerate.
    UnknownLine,
    /// A line the CLI prints before its session opens, carrying `text`.
    BeforeOpen(String),
}

/// What an argv asks of its CLI, as its flags say it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asks {
    /// The sandbox the commands run under. A CLI told no sandbox at all
    /// runs them unconfined, which is `danger-full-access`.
    pub sandbox: Option<String>,
    /// Whether the commands may reach the network.
    pub network: bool,
    /// Each hook the CLI runs, with the seconds the CLI waits for it.
    pub hooks: Vec<(RailEvent, u64)>,
    pub effort: Option<String>,
    pub model: Option<String>,
    /// Whether the CLI loads none of the person's own configuration.
    pub isolated: bool,
    /// The agent's instructions the CLI adds to its own system prompt.
    pub instructions: Option<String>,
}

/// One CLI, or the mock standing for one, as the cases drive it.
#[async_trait::async_trait]
pub trait Subject: Sync {
    /// The name a case reports under: `claude_code`, `codex` or `mock`.
    fn name(&self) -> &'static str;

    /// The adapter this subject plays, built from `settings`.
    fn adapter(&self, settings: AdapterSettings) -> Box<dyn Adapter>;

    /// Plays `script` in `cwd`: the events the adapter emits, and the argv
    /// it was given.
    async fn play(&self, cwd: &Path, req: SessionRequest, script: &Script) -> Played;

    /// What a played session asks of its CLI, read back from its argv and
    /// from the worktree it ran in.
    fn read_back(&self, played: &Played) -> Asks;

    /// The version the subject's CLI reports when it is probed.
    fn version(&self) -> &'static str;

    /// The capabilities the subject's CLI declares.
    fn expected_capabilities(&self) -> Capabilities;

    /// The environment variable that sets `knob` of the scripted CLI, the
    /// name its stub reads it under.
    fn stub_var(&self, knob: &str) -> String;
}

/// The mock adapter, playing each script as a claude-code session.
pub struct MockSubject;

/// The real claude-code adapter, driving its scripted stub.
pub struct ClaudeCodeSubject;

/// The real codex adapter, driving its scripted stub.
pub struct CodexSubject;

#[async_trait::async_trait]
impl Subject for MockSubject {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn adapter(&self, settings: AdapterSettings) -> Box<dyn Adapter> {
        Box::new(MockAdapter::with_settings(mock_fixture(&idle()), settings))
    }

    async fn play(&self, cwd: &Path, req: SessionRequest, script: &Script) -> Played {
        let mock = MockAdapter::new(mock_fixture(script));
        let session = mock.spawn(req).await.expect("the scripted session opens");
        let events = drain(session).await;
        let argv = mock.argvs_seen().into_iter().next().unwrap_or_default();
        let cwd = cwd.to_path_buf();
        Played { events, argv, cwd }
    }

    fn read_back(&self, played: &Played) -> Asks {
        read_claude(played)
    }

    fn version(&self) -> &'static str {
        "mock-0.1"
    }

    fn expected_capabilities(&self) -> Capabilities {
        claude_capabilities()
    }

    fn stub_var(&self, knob: &str) -> String {
        panic!("the mock plays no process, so it has no `{knob}` knob")
    }
}

#[async_trait::async_trait]
impl Subject for ClaudeCodeSubject {
    fn name(&self) -> &'static str {
        "claude_code"
    }

    fn adapter(&self, mut settings: AdapterSettings) -> Box<dyn Adapter> {
        catch_interrupt();
        if settings.binary.is_none() {
            settings.binary = Some(stubs::claude_code());
        }
        Box::new(ClaudeCodeAdapter::new(&settings))
    }

    async fn play(&self, cwd: &Path, req: SessionRequest, script: &Script) -> Played {
        play_stub(self, cwd, req, &wire::claude_lines(script)).await
    }

    fn read_back(&self, played: &Played) -> Asks {
        read_claude(played)
    }

    fn version(&self) -> &'static str {
        "2.1.296"
    }

    fn expected_capabilities(&self) -> Capabilities {
        claude_capabilities()
    }

    fn stub_var(&self, knob: &str) -> String {
        format!("CLAUDE_STUB_{knob}")
    }
}

#[async_trait::async_trait]
impl Subject for CodexSubject {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn adapter(&self, mut settings: AdapterSettings) -> Box<dyn Adapter> {
        catch_interrupt();
        if settings.binary.is_none() {
            settings.binary = Some(stubs::codex());
        }
        Box::new(CodexAdapter::new(&settings))
    }

    async fn play(&self, cwd: &Path, req: SessionRequest, script: &Script) -> Played {
        play_stub(self, cwd, req, &wire::codex_lines(script)).await
    }

    fn read_back(&self, played: &Played) -> Asks {
        read_codex(played)
    }

    fn version(&self) -> &'static str {
        "0.157.0"
    }

    fn expected_capabilities(&self) -> Capabilities {
        codex_capabilities()
    }

    fn stub_var(&self, knob: &str) -> String {
        format!("CODEX_STUB_{knob}")
    }
}

/// What claude-code declares: every capability it has, with the run tools
/// named the way its CLI lists them.
fn claude_capabilities() -> Capabilities {
    Capabilities {
        resume_session: true,
        fence: FenceLevel::ToolCalls,
        permission_profiles: true,
        custom_agents: true,
        usage_reporting: true,
        skills: true,
        run_tools: true,
        network_isolation: true,
        tool_naming: ToolNaming::McpPrefixed,
        stop_refusals: true,
        mid_session_token_cap: true,
    }
}

/// What codex declares: the same set as claude-code, with its run tools
/// named with the server's `-` written `_`, and no mid-session token cap.
fn codex_capabilities() -> Capabilities {
    Capabilities {
        tool_naming: ToolNaming::McpPrefixedUnderscored,
        mid_session_token_cap: false,
        ..claude_capabilities()
    }
}

/// Plays `lines` through the subject's scripted CLI, which streams them from a
/// file and records its argv in another, both reached through the subject's knobs.
async fn play_stub(
    subject: &dyn Subject,
    cwd: &Path,
    mut req: SessionRequest,
    lines: &[String],
) -> Played {
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let lines_file = write_lines(cwd, "contract-lines.jsonl", &borrowed);
    let args_file = cwd.join("contract-args.txt");
    req.env.plain.insert(
        subject.stub_var("LINES_FILE"),
        lines_file.display().to_string(),
    );
    req.env.plain.insert(
        subject.stub_var("ARGS_FILE"),
        args_file.display().to_string(),
    );
    let session = subject
        .adapter(AdapterSettings::default())
        .spawn(req)
        .await
        .expect("the scripted session opens");
    let events = drain(session).await;
    let argv: Vec<String> = std::fs::read_to_string(&args_file)
        .expect("the stub records its argv")
        .lines()
        .map(str::to_string)
        .collect();
    let cwd = cwd.to_path_buf();
    Played { events, argv, cwd }
}

/// The fixture that plays `script` as one session of the claude-code
/// dialect, written as YAML and read back by the mock's own parser.
fn mock_fixture(script: &Script) -> MockFixture {
    let steps: Vec<Value> = script.steps.iter().map(mock_step).collect();
    let mut session = json!({ "steps": steps, "outcome": mock_outcome(&script.outcome) });
    if let Some(model) = &script.model {
        session["model"] = json!(model);
    }
    let document = json!({ "dialect": "claude-code", "sessions": [session] });
    let yaml = yunta_core::yaml::to_string(&document).expect("the script renders as YAML");
    MockFixture::parse_without_a_run(&yaml).expect("the script is a fixture")
}

/// One step of a script as the mock's fixture spells it: a run tool named the
/// way the claude-code dialect names it, and a stop refusal as the CLI's text.
fn mock_step(step: &Step) -> Value {
    match step {
        Step::Note(text) => json!({ "type": "note", "text": text }),
        Step::StopRefused { reason } => {
            json!({ "type": "note", "text": format!("{STOP_TURN}{reason}") })
        }
        Step::ToolUse { name, target } => {
            json!({ "type": "tool_use", "name": name, "target": target })
        }
        Step::RunToolCalled(tool) => json!({
            "type": "tool_use",
            "name": tool.called(claude_capabilities().tool_naming),
            "target": "",
        }),
        Step::Usage {
            input,
            output,
            cached,
        } => json!({
            "type": "usage",
            "input_tokens": input,
            "output_tokens": output,
            "cached_input_tokens": cached,
        }),
        Step::UnknownLine => json!({ "type": "unknown_line" }),
        Step::BeforeOpen(text) => json!({ "type": "before_open", "text": text }),
    }
}

/// A script's outcome as the mock's fixture spells it.
fn mock_outcome(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Completed(summary) => json!({ "type": "completed", "summary": summary }),
        Outcome::Failed(message) => json!({ "type": "failed", "message": message }),
        Outcome::Crash => json!({ "type": "crash" }),
    }
}

/// A script that completes at once, for a subject asked only for its adapter.
fn idle() -> Script {
    Script {
        model: None,
        steps: Vec::new(),
        outcome: Outcome::Completed("ok".to_string()),
    }
}

/// What a claude-code session asks, read back from its flags and from the
/// `--settings` document that carries its hooks and sandbox.
///
/// A CLI given no sandbox runs its commands unconfined, with the network open.
/// A sandbox that denies the worktree is `read-only`; any other is `workspace-write`.
fn read_claude(played: &Played) -> Asks {
    let argv = &played.argv;
    let settings: Value = serde_json::from_str(
        flag_value(argv, "--settings").expect("the hooks travel in --settings"),
    )
    .expect("--settings is JSON");
    let sandbox = settings.get("sandbox");
    let worktree = json!(played.cwd);
    let denies_worktree = sandbox
        .and_then(|sandbox| sandbox.pointer("/filesystem/denyWrite"))
        .and_then(Value::as_array)
        .is_some_and(|denied| denied.contains(&worktree));
    let mode = match sandbox {
        None => "danger-full-access",
        Some(_) if denies_worktree => "read-only",
        Some(_) => "workspace-write",
    };
    let network =
        sandbox.is_none_or(|sandbox| sandbox.pointer("/network/allowedDomains").is_some());
    let hooks: Vec<(RailEvent, u64)> = HOOK_EVENTS
        .iter()
        .filter_map(|(key, event)| {
            let group = settings.get("hooks")?.get(*key)?.get(0)?;
            let timeout = group.get("hooks")?.get(0)?.get("timeout")?.as_u64()?;
            Some((*event, timeout))
        })
        .collect();
    let instructions = flag_value(argv, "--append-system-prompt-file")
        .map(|path| std::fs::read_to_string(path).expect("the agent's instructions were written"));
    Asks {
        sandbox: Some(mode.to_string()),
        network,
        hooks,
        effort: flag_value(argv, "--effort").map(str::to_string),
        model: flag_value(argv, "--model").map(str::to_string),
        isolated: argv.windows(2).any(
            |pair| matches!(pair, [flag, value] if flag == "--setting-sources" && value.is_empty()),
        ) && argv.iter().any(|arg| arg == "--strict-mcp-config"),
        instructions,
    }
}

/// What a codex session asks, read back from its flags and from its `-c`
/// overrides, which are TOML assignments read together as one document.
///
/// A confined session names its permissions profile, whose base says what its
/// commands write and whose switch says whether they reach the network; an
/// unconfined one passes `--sandbox danger-full-access`.
fn read_codex(played: &Played) -> Asks {
    let argv = &played.argv;
    let overrides: Vec<&str> = argv
        .windows(2)
        .filter_map(|pair| match pair {
            [flag, value] if flag == "-c" => Some(value.as_str()),
            _ => None,
        })
        .collect();
    let config: toml::Table =
        toml::from_str(&overrides.join("\n")).expect("the -c overrides read as TOML");
    let text = |key: &str| {
        config
            .get(key)
            .and_then(toml::Value::as_str)
            .map(str::to_string)
    };
    let hooks_table = config.get("hooks").and_then(toml::Value::as_table);
    let hooks = HOOK_EVENTS
        .iter()
        .filter_map(|(key, event)| {
            let group = hooks_table?.get(*key)?.as_array()?.first()?.as_table()?;
            let handler = group.get("hooks")?.as_array()?.first()?.as_table()?;
            let timeout = handler.get("timeout")?.as_integer()?;
            Some((*event, u64::try_from(timeout).ok()?))
        })
        .collect();
    let permissions = config.get("permissions");
    let profile = permissions.and_then(|table| table.get("yunta-session"));
    let enabled = profile
        .and_then(|table| table.get("network")?.get("enabled"))
        .and_then(toml::Value::as_bool);
    let base = profile.and_then(|table| table.get("extends"));
    let sandbox = flag_value(argv, "--sandbox").or(match base.and_then(toml::Value::as_str) {
        Some(":read-only") => Some("read-only"),
        Some(":workspace") => Some("workspace-write"),
        _ => None,
    });
    Asks {
        sandbox: sandbox.map(str::to_string),
        // A session with no profile is the unconfined one, and its network is open.
        network: enabled.unwrap_or(true),
        hooks,
        effort: text("model_reasoning_effort"),
        model: flag_value(argv, "--model").map(str::to_string),
        isolated: argv.iter().any(|arg| arg == "--ignore-user-config")
            && argv.iter().any(|arg| arg == "--ignore-rules"),
        instructions: text("developer_instructions"),
    }
}

/// The value of the flag `name` in `argv`: the argument after it.
fn flag_value<'a>(argv: &'a [String], name: &str) -> Option<&'a str> {
    let at = argv.iter().position(|arg| arg == name)?;
    argv.get(at + 1).map(String::as_str)
}

/// The SIGINT listener this binary keeps for its whole life.
static INTERRUPT: Mutex<Option<Signal>> = Mutex::new(None);

/// Catches SIGINT in this process before a real CLI is spawned from it.
///
/// A test binary started with SIGINT ignored hands that ignore to every child,
/// and a child cannot trap a signal it inherits as ignored. A caught signal is
/// reset to its default at exec, so the CLI meets SIGINT as it does under the
/// engine. The listener lives in a static, so it outlasts every test.
fn catch_interrupt() {
    let mut held = INTERRUPT.lock().unwrap_or_else(PoisonError::into_inner);
    if held.is_none() {
        *held = Some(signal(SignalKind::interrupt()).expect("this process can catch SIGINT"));
    }
}
