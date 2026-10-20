//! The real `codex` adapter: spawns `codex exec --json` headless,
//! streams its JSONL into `AgentEvent`s (`parse.rs`), and maps yunta's
//! portable request fields onto the CLI's own flags (`plan`). Automated
//! tests use a scripted CLI. A live Codex probe is run separately when the
//! binary and credentials are available.
//!
//! **`resume_session` is a fixed `true`, not computed from `probe()`.**
//! `Adapter::new` is synchronous and `capabilities()` is fixed at
//! construction, with no I/O needed to report it, so it has no access to an
//! async probe result. `claude_code` does not attempt this either.
//! `codex exec resume` is a documented, stable subcommand at the CLI version
//! this was written against, so declaring the capability unconditionally is
//! accurate at that version; version-gating it for real would need an async
//! constructor threaded through `real_adapters()`, a bigger change than this
//! one adapter justifies on its own.

mod config;
mod parse;
mod profile;
pub(crate) mod rail;
pub(crate) mod settings;
pub(crate) mod wire;

pub(crate) use parse::failure_cause;

use std::path::PathBuf;

use async_trait::async_trait;
use yunta_core::fence::Coverage;
use yunta_core::port::session::SessionEnv;
use yunta_core::port::{
    Adapter, AgentEvent, AgentSession, ProbeReport, RunToolsEndpoint, SessionRequest,
};
use yunta_core::process::subprocess::{self, Launch, LineParser};
use yunta_core::rail::RailCommand;
use yunta_core::{
    AdapterError, AdapterId, AdapterSettings, Capabilities, EditSandbox, FenceLevel, Result,
    SessionId, ToolNaming,
};

use crate::dialect::{require_hook, Plan};
use crate::mount::MountPlan;
use config::ConfigOverride;

/// The id config names this adapter by.
pub static ID: AdapterId = AdapterId::from_static("codex");

/// Where the CLI discovers a project's skills, relative to the session's
/// working directory.
const SKILLS_MOUNT: &str = ".agents/skills";

/// The variable the CLI reads the per-run bearer token from. Naming the
/// variable in the config, rather than inlining the token, is what keeps
/// the credential out of `argv` — which `ps` shows to any local process.
const TOKEN_VAR: &str = "YUNTA_RUN_TOOLS_TOKEN";

pub struct CodexAdapter {
    binary: PathBuf,
    /// The typed reading of `adapter_settings`, or the error it produced —
    /// reported by `probe()`, where a misconfiguration is diagnosed before
    /// any session is opened.
    settings: Result<settings::CodexSettings>,
    /// The settings as written, which every launch plans from.
    raw: AdapterSettings,
}

impl CodexAdapter {
    pub fn new(settings: &AdapterSettings) -> Self {
        Self {
            binary: settings
                .binary
                .clone()
                .unwrap_or_else(|| PathBuf::from("codex")),
            settings: settings::CodexSettings::read(settings),
            raw: settings.clone(),
        }
    }

    async fn launch(
        &self,
        req: SessionRequest,
        resume: Option<&SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        let plan = plan(&self.raw, &req, resume)?;
        plan.write_scratch(&ID)?;
        let mounts = plan.mounts.install(&ID)?;
        subprocess::open(Launch {
            adapter: &ID,
            binary: &self.binary,
            args: plan.args,
            cwd: &req.cwd,
            env: &plan.env,
            prompt: &req.prompt,
            parser: Box::new(CodexParser::new(plan.coverage)),
            held: Some(Box::new(mounts)),
        })
        .await
    }
}

/// What one launch of this CLI asks for, as data, before anything is
/// written: its arguments, environment, scratch files, mounts, and the
/// coverage its sandbox leaves. Pure, so the mock plans its sessions
/// the same way a real launch does.
pub(crate) fn plan(
    settings: &AdapterSettings,
    req: &SessionRequest,
    resume: Option<&SessionId>,
) -> Result<Plan> {
    // Settings that do not read fail the session rather than fall back: a
    // `sandbox:` nobody could parse would run the agent under a confinement
    // the team never asked for, and silently.
    settings::CodexSettings::read(settings).map_err(|unreadable| {
        AdapterError::UnreadableSettings {
            adapter: ID.clone(),
            detail: unreadable.to_string(),
        }
    })?;
    // The engine judges this CLI's edits only through a hook it can run and
    // a listener the hook can reach. Without both there is no fence to build,
    // and a session that opens anyway writes wherever its sandbox lets it.
    let (command, _) = require_hook(&ID, req)?;
    let overrides = profile::overrides(&req.confinement, &req.cwd).map_err(|source| {
        AdapterError::FenceUnbuildable {
            adapter: ID.clone(),
            source,
        }
    })?;
    Ok(Plan {
        args: argv(req, resume, command, overrides),
        env: session_env(req),
        scratch: Vec::new(),
        mounts: MountPlan::of(SKILLS_MOUNT, req),
        coverage: req.confinement.coverage(&req.cwd),
    })
}

/// The arguments one launch passes: the run's flags and configuration, then
/// the resume, then the marker that reads the prompt from stdin.
fn argv(
    req: &SessionRequest,
    resume: Option<&SessionId>,
    command: &RailCommand,
    profile_args: Vec<String>,
) -> Vec<String> {
    let mut args = vec!["exec".to_string(), "--json".to_string()];
    if let Some(model) = &req.model {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    args.extend(profile_args);
    args.extend(config_overrides(req));
    args.extend(rail::hook_overrides(
        command,
        RunToolsEndpoint::call_timeout(&req.budget),
        req.formats,
    ));
    // An isolated session loads none of the person's own configuration
    // or rules; its credentials stay theirs.
    if req.cli_config == yunta_core::CliConfig::Isolated {
        args.push("--ignore-user-config".to_string());
        args.push("--ignore-rules".to_string());
    }
    // `exec`'s own options are declared on the parent command and
    // are not `global`, so clap reads one that follows `resume` as
    // an unexpected argument and the invocation dies before a
    // session opens: the subcommand goes last, with only its own
    // arguments after it.
    if let Some(session_id) = resume {
        args.push("resume".to_string());
        args.push(session_id.as_str().to_string());
    }
    // `-` makes the CLI read the prompt from stdin, so nothing of
    // it shows in the process list.
    args.push("-".to_string());
    args
}

/// The environment one session's CLI runs in: the request's own, the
/// credential the config names, and where the hooks reach the engine.
/// Credentials go in `secret`, which a log never carries; the rail's
/// address is not one and goes in `plain`.
fn session_env(req: &SessionRequest) -> SessionEnv {
    let mut env = req.env.clone();
    // The credential the config names, placed where a secret is allowed
    // to travel: the child's own environment.
    if let Some(endpoint) = &req.run_tools_endpoint {
        env.secret
            .insert(TOKEN_VAR.to_string(), endpoint.token.clone());
    }
    // Where the hooks reach the engine, inherited by every hook this CLI
    // runs.
    if let Some(rail) = &req.rail {
        rail.hand_to(&mut env);
    }
    env
}

/// The `-c` overrides one request needs, in the order they are written.
///
/// The effort, the agent's instructions and the run's own tools are
/// settings of the CLI's config file, which `-c` overrides for this
/// invocation alone rather than writing to the user's own
/// `~/.codex/config.toml`.
fn config_overrides(req: &SessionRequest) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(effort) = req.effort {
        args.extend(ConfigOverride::string("model_reasoning_effort", effort.as_str()).into_args());
    }
    // The agent's instructions join the CLI's own, the key a profile
    // carries them in.
    if let Some(agent) = &req.agent {
        args.extend(
            ConfigOverride::string("developer_instructions", &agent.instructions).into_args(),
        );
    }
    if let Some(endpoint) = &req.run_tools_endpoint {
        args.extend(run_tools_overrides(&endpoint.url, &req.budget));
    }
    args
}

/// The per-run server reaches the CLI as an external MCP server over
/// streamable HTTP: `url` is the key that selects that transport, and the
/// credential travels as the name of the variable the CLI reads it from.
fn run_tools_overrides(url: &str, budget: &yunta_core::port::Budget) -> Vec<String> {
    let server = RunToolsEndpoint::SERVER_NAME;
    [
        ConfigOverride::string(format!("mcp_servers.{server}.url"), url),
        ConfigOverride::string(
            format!("mcp_servers.{server}.bearer_token_env_var"),
            TOKEN_VAR,
        ),
        ConfigOverride::string(
            format!("mcp_servers.{server}.default_tools_approval_mode"),
            "approve",
        ),
        // A call can run a task's criteria; the CLI waits for it as long
        // as the session may run.
        ConfigOverride::float(
            format!("mcp_servers.{server}.tool_timeout_sec"),
            RunToolsEndpoint::call_timeout(budget).as_secs_f64(),
        ),
    ]
    .into_iter()
    .flat_map(ConfigOverride::into_args)
    .collect()
}

/// The CLI's JSONL, one event list per line. The last note seen is what a
/// turn's completion reports as its summary, so it travels from line to line.
pub(crate) struct CodexParser {
    last_message: String,
    /// What the sandbox this session runs under actually fenced — computed
    /// once, when the session was planned.
    fence: Coverage,
}

impl CodexParser {
    pub(crate) fn new(fence: Coverage) -> Self {
        Self {
            last_message: String::new(),
            fence,
        }
    }
}

impl LineParser for CodexParser {
    fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        let events = parse::parse_line(line, &self.last_message, &self.fence);
        if let Some(text) = events.iter().rev().find_map(|event| match event {
            AgentEvent::Note { text } => Some(text.clone()),
            _ => None,
        }) {
            self.last_message = text;
        }
        events
    }
}

/// What this CLI declares, which the adapter answers and the mock plays.
pub(crate) fn capabilities() -> Capabilities {
    Capabilities {
        resume_session: true,
        // Mounted by each launch into the session cwd's own
        // `.agents/skills/` — where the CLI discovers a project's skills.
        skills: true,
        // Every edit goes through a `PreToolUse` hook that asks the
        // engine before the patch applies; commands stay confined by the
        // process's sandbox, which the coverage reports.
        fence: FenceLevel::ToolCalls,
        permission_profiles: true,
        // A named agent's instructions reach the CLI the way a profile
        // carries them, as `developer_instructions`.
        custom_agents: true,
        usage_reporting: true,
        // The per-run MCP server reaches the CLI as an external
        // streamable-HTTP server, configured by `-c` overrides.
        // Like every other mapping in this adapter, this is read
        // off the CLI's own source rather than confirmed live —
        // see this module's doc comment.
        run_tools: true,
        // The session's profile keeps it off the network unless its
        // request opens it; an unconfined session cannot close it, so
        // the confinement refuses one whose network is closed.
        network_isolation: true,
        // The CLI names an MCP server's tools `mcp__<server>__<tool>`
        // with the server's `-` written `_`: a session finds
        // `mcp__yunta_run__yunta_task`, never `yunta_task`.
        tool_naming: ToolNaming::McpPrefixedUnderscored,
        // The CLI takes another turn after each stop the engine refuses,
        // over whatever state the work is in.
        stop_refusals: true,
        // The CLI reports a turn's usage when the turn completes, so the
        // token cap is checked when the session completes, not line by line.
        mid_session_token_cap: false,
    }
}

/// The trees of a checkout this CLI loads its configuration from: the
/// `.codex/` tree, with its config and hooks, and the skills root under
/// `.agents/`, which is all of that tree it scans. A tree covers every file
/// under it, including files a session creates later.
pub(crate) fn own_config() -> Vec<yunta_core::ScopeGlob> {
    [".codex/**".to_string(), format!("{SKILLS_MOUNT}/**")]
        .iter()
        .filter_map(|tree| tree.parse().ok())
        .collect()
}

/// The `sandbox` an `edit` session's commands run under, as the settings
/// name it: `workspace-write` when they name none. A setting that does not
/// read is refused when the session is planned.
pub(crate) fn edit_sandbox(settings: &AdapterSettings) -> EditSandbox {
    settings::CodexSettings::read(settings)
        .ok()
        .and_then(|typed| typed.sandbox)
        .unwrap_or_default()
}

#[async_trait]
impl Adapter for CodexAdapter {
    fn id(&self) -> &'static AdapterId {
        &ID
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn staged_paths(&self, req: &SessionRequest) -> Vec<PathBuf> {
        plan(&self.raw, req, None)
            .map(|plan| plan.mounts.staged())
            .unwrap_or_default()
    }

    fn rail_codec(&self) -> Option<&dyn yunta_core::port::RailCodec> {
        Some(&rail::CodexRailCodec)
    }

    async fn probe(&self) -> Result<ProbeReport> {
        if let Err(e) = &self.settings {
            return Err(AdapterError::Adapter {
                adapter: ID.clone(),
                message: e.to_string(),
            });
        }
        Ok(profile::held_to_floor(
            subprocess::probe_version(&self.binary).await,
        ))
    }

    fn own_config(&self) -> Vec<yunta_core::ScopeGlob> {
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
            "OPENAI_BASE_URL",
        ]
    }

    /// The OpenAI API, or the endpoint or proxy the session is pointed at.
    async fn reachable(
        &self,
        req: &SessionRequest,
        shell: &std::collections::BTreeMap<String, String>,
    ) -> Option<bool> {
        crate::reach::answers(req, shell, "OPENAI_BASE_URL", "api.openai.com").await
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
