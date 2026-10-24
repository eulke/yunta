//! The real `claude-code` adapter: spawns the `claude` CLI headless
//! (`-p --output-format stream-json`), streams its JSON lines into
//! `AgentEvent`s (`parse.rs`), and maps yunta's portable request fields
//! onto the CLI's own flags (`permissions.rs`).
//!
//! Its fence judges each tool call: the `--settings` a launch passes install
//! the hooks that ask the engine before every writing tool, before the
//! session stops, and after a write the run formats (`rail.rs`), and the
//! fence's roots reach the CLI as `--add-dir`. The same settings hold the
//! sandbox a confined session's commands run in, which fails the session
//! rather than run a command unconfined. An agent's instructions are read
//! from a file of the session's scratch, and the run's tools are an HTTP
//! server of the session's `--mcp-config` file, which keeps the bearer off
//! `argv`.
//!
//! Never exercised by the automated suite — no real LLM in CI — covered
//! instead by `crates/adapters/tests/claude_code.rs` and the contract suite
//! against a scripted fake binary, plus one check run by hand against the
//! real CLI; cargo test runs the stubs only.

mod parse;
mod permissions;
pub(crate) mod rail;
pub(crate) mod settings;
pub(crate) mod wire;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use yunta_core::{
    AdapterError, AdapterId, AdapterSettings, Capabilities, EditSandbox, FenceLevel, Result,
    ScopeGlob, SessionId, ToolNaming,
};

use yunta_core::fence::Coverage;
use yunta_core::port::session::SessionEnv;
use yunta_core::port::{
    Adapter, AgentEvent, AgentSession, ProbeReport, RunToolsEndpoint, SessionRequest,
};
use yunta_core::process::subprocess::{self, Launch, LineParser};
use yunta_core::rail::RailCommand;

use crate::dialect::{require_hook, Plan};
use crate::mount::MountPlan;

/// The id config names this adapter by.
pub static ID: AdapterId = AdapterId::from_static("claude-code");

/// The CLI's native skills discovery directory, relative to the session's
/// working directory.
const SKILLS_MOUNT: &str = ".claude/skills";

/// The file, in a session's scratch directory, that holds the agent's
/// instructions.
const AGENT_FILE: &str = "agent.md";

/// The variable the CLI reads its cap on how many times in a row a stop
/// hook may keep the session working from; `0` lifts it.
const STOP_BLOCK_CAP: &str = "CLAUDE_CODE_STOP_HOOK_BLOCK_CAP";

/// One launch as data, planned without writing anything: what the CLI is
/// asked, the environment it runs in, the files it reads from its scratch
/// directory, the mounts it stages, and how much of the session the fence
/// covers. The real launch and the mock both plan a session this way.
pub(crate) fn plan(
    settings: &AdapterSettings,
    req: &SessionRequest,
    resume: Option<&SessionId>,
) -> Result<Plan> {
    read_settings(settings)?;
    let (command, _) = require_hook(&ID, req)?;
    let conf = &req.confinement;
    let sandbox =
        permissions::settings(conf, &req.cwd).map_err(|source| AdapterError::FenceUnbuildable {
            adapter: ID.clone(),
            source,
        })?;
    Ok(Plan {
        args: launch_args(req, resume, command, sandbox),
        env: session_env(req),
        scratch: scratch_files(req)?,
        mounts: MountPlan::of(SKILLS_MOUNT, req),
        coverage: conf.coverage(&req.cwd),
    })
}

/// The typed settings a session is planned under. Settings that do not
/// read refuse the session: a session opened under settings nobody could
/// parse runs under something nobody asked for, and silently.
fn read_settings(settings: &AdapterSettings) -> Result<settings::ClaudeSettings> {
    settings::ClaudeSettings::read(settings).map_err(|unreadable| {
        AdapterError::UnreadableSettings {
            adapter: ID.clone(),
            detail: unreadable.to_string(),
        }
    })
}

/// The files a launch writes into the session's scratch directory, with the
/// bytes each holds: the per-run tool config when there is a server to
/// reach, and the agent's instructions when the session works as an agent.
fn scratch_files(req: &SessionRequest) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut files = Vec::new();
    if let Some(endpoint) = &req.run_tools_endpoint {
        files.push((mcp_config_path(req), mcp_config_bytes(req, endpoint)?));
    }
    if let Some(agent) = &req.agent {
        files.push((agent_path(req), agent.instructions.as_bytes().to_vec()));
    }
    Ok(files)
}

/// Where a session's per-run tool config is written. It goes to the
/// session's own scratch directory, never the worktree: the worktree's diff
/// is what the engine's scope check reads, and a file this adapter dropped
/// there would read as the agent's own work. That directory is this
/// session's alone, so the file is named for what it is rather than for
/// anything that makes it unique.
fn mcp_config_path(req: &SessionRequest) -> PathBuf {
    req.scratch_dir.join(RunToolsEndpoint::CONFIG_FILE)
}

fn agent_path(req: &SessionRequest) -> PathBuf {
    req.scratch_dir.join(AGENT_FILE)
}

/// The per-run tool config a session's CLI reads, as the bytes to write.
/// The bearer token travels in the file rather than on the command line —
/// `argv` is world-readable through `ps`, and the token is the only thing
/// standing between any local process and this node's tools. The file is
/// written whole and private from its first byte, so no other user reads
/// the token, not even for a moment.
fn mcp_config_bytes(req: &SessionRequest, endpoint: &RunToolsEndpoint) -> Result<Vec<u8>> {
    let config = serde_json::json!({
        "mcpServers": {
            RunToolsEndpoint::SERVER_NAME: {
                "type": "http",
                "url": endpoint.url,
                "headers": { "Authorization": format!("Bearer {}", endpoint.token.expose()) },
                // A call can run a task's criteria; the CLI waits for it
                // as long as the session may run, in milliseconds.
                "timeout": RunToolsEndpoint::call_timeout(&req.budget).as_millis(),
            }
        }
    });
    serde_json::to_vec_pretty(&config).map_err(|source| AdapterError::Adapter {
        adapter: ID.clone(),
        message: format!("could not render the per-run tool config: {source}"),
    })
}

/// The arguments of one launch. The prompt is not among them: it arrives on
/// stdin, where `-p` reads it, so nothing of it shows in the process list.
fn launch_args(
    req: &SessionRequest,
    resume: Option<&SessionId>,
    command: &RailCommand,
    sandbox: Option<serde_json::Value>,
) -> Vec<String> {
    let mut args = session_args(req, resume);
    args.extend(permissions::permission_args(
        req.confinement.profile,
        !req.confinement.roots.is_empty(),
    ));
    args.extend(fence_args(req, command, sandbox));
    // An isolated session loads none of the person's own settings, and no
    // MCP server but the run's; its credentials stay theirs.
    if req.cli_config == yunta_core::CliConfig::Isolated {
        args.push("--setting-sources".to_string());
        args.push(String::new());
        args.push("--strict-mcp-config".to_string());
    }
    if req.run_tools_endpoint.is_some() {
        args.extend(run_tools_args(&mcp_config_path(req)));
    }
    args
}

/// The arguments that open the CLI's stream, and say which session, model,
/// effort and agent it runs under.
fn session_args(req: &SessionRequest, resume: Option<&SessionId>) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ];
    if let Some(session_id) = resume {
        args.push("--resume".to_string());
        args.push(session_id.as_str().to_string());
    }
    if let Some(model) = &req.model {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    if let Some(effort) = req.effort {
        args.push("--effort".to_string());
        args.push(effort.as_str().to_string());
    }
    // The agent's instructions join the CLI's own, read from the file the
    // session's scratch holds.
    if req.agent.is_some() {
        args.push("--append-system-prompt-file".to_string());
        args.push(agent_path(req).display().to_string());
    }
    args
}

/// How the fence reaches this CLI: the roots the confinement keeps writable,
/// and the engine as the hook it asks before every writing tool.
///
/// The CLI confines file writes to its working directory, and a root the
/// confinement keeps writable is never inside it — without this the session
/// is told to write a file it is then refused permission to create. The
/// same settings carry the sandbox its commands run in. `plan` refuses
/// before this runs when there is no hook to run, so a session never opens
/// without one.
fn fence_args(
    req: &SessionRequest,
    command: &RailCommand,
    sandbox: Option<serde_json::Value>,
) -> Vec<String> {
    let mut args = Vec::new();
    for root in &req.confinement.roots {
        args.push("--add-dir".to_string());
        args.push(root.display().to_string());
    }
    args.push("--settings".to_string());
    args.push(rail::settings_json(
        command,
        sandbox,
        RunToolsEndpoint::call_timeout(&req.budget),
        req.formats,
    ));
    args
}

/// The environment one session's CLI runs in: the request's own, where the
/// hooks reach the engine, and the stop cap. The address is not a secret;
/// the credential is, and goes in `secret`.
fn session_env(req: &SessionRequest) -> SessionEnv {
    let mut env = req.env.clone();
    // Where the hooks reach the engine travels in the child's own
    // environment, which is how a hook this CLI runs inherits it.
    if let Some(rail) = &req.rail {
        rail.hand_to(&mut env);
    }
    // The engine sends a stop back at most once for one state of the
    // work, so the CLI's own cap on how often a stop hook may do so
    // would only cut that judgement short.
    env.plain
        .insert(STOP_BLOCK_CAP.to_string(), "0".to_string());
    env
}

/// The CLI's stream-json lines, one event list per line.
pub(crate) struct ClaudeParser {
    fence: Coverage,
    run_tool_calls: std::collections::HashMap<String, yunta_core::RunTool>,
    reported: parse::Reported,
    /// What the stream's limit fields have said so far.
    limit: parse::LimitReading,
}

impl ClaudeParser {
    /// A reader for a session whose fence covers `fence`.
    pub(crate) fn new(fence: Coverage) -> Self {
        ClaudeParser {
            fence,
            run_tool_calls: std::collections::HashMap::new(),
            reported: parse::Reported::default(),
            limit: parse::LimitReading::default(),
        }
    }
}

impl LineParser for ClaudeParser {
    fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        parse::parse_line(
            line,
            Some(&self.fence),
            &mut self.run_tool_calls,
            &mut self.reported,
            &mut self.limit,
        )
    }
}

pub struct ClaudeCodeAdapter {
    binary: PathBuf,
    /// The settings as the config wrote them. `plan` reads them the way a
    /// session does, so the real launch and the mock plan alike.
    raw: AdapterSettings,
    /// The typed reading of `adapter_settings`, or the error it
    /// produced — reported by `probe()`, where a misconfiguration is
    /// diagnosed before any session is opened.
    settings: Result<settings::ClaudeSettings>,
}

impl ClaudeCodeAdapter {
    pub fn new(settings: &AdapterSettings) -> Self {
        Self {
            binary: settings
                .binary
                .clone()
                .unwrap_or_else(|| PathBuf::from("claude")),
            raw: settings.clone(),
            settings: settings::ClaudeSettings::read(settings),
        }
    }

    async fn launch(
        &self,
        req: SessionRequest,
        resume: Option<&SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        let planned = plan(&self.raw, &req, resume)?;
        planned.write_scratch(&ID)?;
        let mounts = planned.mounts.install(&ID)?;
        subprocess::open(Launch {
            adapter: &ID,
            binary: &self.binary,
            args: planned.args,
            cwd: &req.cwd,
            env: &planned.env,
            prompt: &req.prompt,
            parser: Box::new(ClaudeParser::new(planned.coverage)),
            held: Some(Box::new(mounts)),
        })
        .await
    }
}

/// What this adapter declares about the CLI it drives. The mock answers
/// with the same set for the same dialect, so a test that narrows it is
/// narrowing what this adapter does.
pub(crate) fn capabilities() -> Capabilities {
    Capabilities {
        resume_session: true,
        // Every writing tool goes through a `PreToolUse` hook that
        // runs the judge before the write happens.
        fence: FenceLevel::ToolCalls,
        permission_profiles: true,
        custom_agents: true,
        usage_reporting: true,
        // The per-run MCP server reaches the CLI as an external
        // HTTP server (`--mcp-config`), the same mechanism a user
        // configures by hand.
        run_tools: true,
        // Mounted by staging into the session cwd's own
        // `.claude/skills/` — the CLI's native discovery location.
        skills: true,
        // Commands run in the CLI's own sandbox, which keeps the
        // network closed unless the session's request opens it.
        network_isolation: true,
        // The stop hook answers a refused stop with the reason to keep
        // working, and the CLI's own cap on how often that may happen is
        // lifted (see `session_env`), so two refusals in a row over
        // different states both reach the CLI.
        stop_refusals: true,
        // The CLI lists an MCP server's tools as `mcp__<server>__<tool>`
        // in its own init line, and that is the only name a call
        // reaches them by.
        tool_naming: ToolNaming::McpPrefixed,
        // Each assistant message reports its usage as the session spends
        // it, so the token cap is checked as the stream runs.
        mid_session_token_cap: true,
    }
}

/// The trees of a checkout this CLI loads configuration from that runs
/// code or mounts tools: `.claude/`, with its settings and their hooks,
/// and the MCP servers in `.mcp.json`. A tree covers every file under
/// it, including files a session creates later.
pub(crate) fn own_config() -> Vec<ScopeGlob> {
    [".claude/**", ".mcp.json"]
        .into_iter()
        .filter_map(|tree| tree.parse().ok())
        .collect()
}

/// The `sandbox` an `edit` session's commands run under, as the settings
/// name it: `workspace-write` when they name none. A setting that does not
/// read is refused when the session is planned.
pub(crate) fn edit_sandbox(settings: &AdapterSettings) -> EditSandbox {
    settings::ClaudeSettings::read(settings)
        .ok()
        .and_then(|typed| typed.sandbox)
        .unwrap_or_default()
}

#[async_trait]
impl Adapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static AdapterId {
        &ID
    }

    fn staged_paths(&self, req: &SessionRequest) -> Vec<PathBuf> {
        plan(&self.raw, req, None)
            .map(|planned| planned.mounts.staged())
            .unwrap_or_default()
    }

    fn rail_codec(&self) -> Option<&dyn yunta_core::port::RailCodec> {
        Some(&rail::ClaudeRailCodec)
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    async fn probe(&self) -> Result<ProbeReport> {
        if let Err(e) = &self.settings {
            return Err(AdapterError::Adapter {
                adapter: ID.clone(),
                message: e.to_string(),
            });
        }
        Ok(subprocess::probe_version(&self.binary).await)
    }

    fn own_config(&self) -> Vec<ScopeGlob> {
        own_config()
    }

    fn edit_sandbox(&self) -> EditSandbox {
        edit_sandbox(&self.raw)
    }

    fn service_vars(&self) -> &'static [&'static str] {
        &[
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
        ]
    }

    /// The Anthropic API, or the gateway or proxy the session is pointed
    /// at. A session routed through a cloud provider's own endpoint is
    /// one this cannot locate.
    async fn reachable(
        &self,
        req: &SessionRequest,
        shell: &std::collections::BTreeMap<String, String>,
    ) -> Option<bool> {
        let routed = ["CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX"]
            .into_iter()
            .any(|name| crate::reach::var(req, shell, name).is_some_and(|value| value != "0"));
        if routed {
            return None;
        }
        crate::reach::answers(req, shell, "ANTHROPIC_BASE_URL", "api.anthropic.com").await
    }

    async fn spawn(&self, req: SessionRequest) -> Result<Box<dyn AgentSession>> {
        self.launch(req, None).await
    }

    async fn resume(
        &self,
        session: &SessionId,
        req: SessionRequest,
    ) -> Result<Box<dyn AgentSession>> {
        self.launch(req, Some(session)).await
    }
}

/// The flags that connect the session to the run's tools, configured in
/// the file at `mcp_config`. `--tools` selects among built-ins only; an
/// MCP server's tools are reached by name. The wildcard admits every tool
/// of the one server, which keeps this adapter from having to know which
/// tools the engine mounts — a bare server prefix names no tool at all,
/// and the CLI drops such a rule with a startup warning.
fn run_tools_args(mcp_config: &Path) -> [String; 4] {
    [
        "--mcp-config".to_string(),
        mcp_config.display().to_string(),
        "--allowedTools".to_string(),
        format!("mcp__{}__*", RunToolsEndpoint::SERVER_NAME),
    ]
}
