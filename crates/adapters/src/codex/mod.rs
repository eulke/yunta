//! The real `codex` adapter: spawns `codex exec --json` headless,
//! streams its JSONL into `AgentEvent`s (`parse.rs`), and maps yunta's
//! portable request fields onto the CLI's own flags (`permissions.rs`).
//! Automated tests use a scripted CLI. A live Codex probe is run
//! separately when the binary and credentials are available.
//!
//! **`resume_session` is a fixed `true`, not literally "calculated in
//! the constructor from `probe()`"** the way the adapter spec's own
//! prose for `codex` reads. `Adapter::new` is synchronous and
//! `capabilities()` — fixed at construction, with no I/O needed to
//! report it — has no `&self` access to an async probe result —
//! `claude_code` doesn't attempt this either, despite the same general
//! framing applying to it too. `codex exec resume` is a documented,
//! stable subcommand at the CLI version this was written against, so
//! declaring the capability unconditionally is accurate today;
//! version-gating it for real would need an async constructor threaded
//! through `real_adapters()`, a bigger change than this one adapter
//! justifies on its own.

mod config;
mod fence;
mod parse;
mod rail;
mod settings;

use std::path::PathBuf;

use async_trait::async_trait;
use yunta_core::{
    AdapterError, AdapterId, AdapterSettings, Capabilities, FenceLevel, Result, SessionId,
    ToolNaming, Unbuildable,
};

use yunta_core::fence::Coverage;
use yunta_core::port::{
    Adapter, AgentEvent, AgentSession, ProbeReport, RunToolsEndpoint, SessionRequest,
};
use yunta_core::process::subprocess::{self, Launch, LineParser};

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
    /// The typed reading of `adapter_settings`, or the error it
    /// produced — reported by `probe()`, where a misconfiguration is
    /// diagnosed before any session is opened. A session opened without
    /// a probe reads the defaults.
    settings: Result<settings::CodexSettings>,
}

impl CodexAdapter {
    pub fn new(settings: &AdapterSettings) -> Self {
        Self {
            binary: settings
                .binary
                .clone()
                .unwrap_or_else(|| PathBuf::from("codex")),
            settings: settings::CodexSettings::read(settings),
        }
    }

    fn build_args(
        &self,
        req: &SessionRequest,
        resume: Option<&SessionId>,
    ) -> std::result::Result<Vec<String>, Unbuildable> {
        let mut args = vec!["exec".to_string(), "--json".to_string()];
        if let Some(model) = &req.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        // `launch` refused already for settings that do not read, so
        // the fallback here is a node whose settings named nothing.
        // The engine judges this CLI's edits only through a hook it can
        // run and a listener the hook can reach. Without both there is no
        // fence to build, and a session that opens anyway writes wherever
        // its sandbox lets it.
        let Some(command) = req.rail_command.as_ref().filter(|_| req.rail.is_some()) else {
            return Err(Unbuildable::HookUnavailable);
        };
        args.extend(self.confinement_args(req));
        args.extend(rail::hook_overrides(
            command,
            RunToolsEndpoint::call_timeout(&req.budget),
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
        // `codex exec` exposes no cap on turns: `budget.max_turns` is
        // bounded here by the engine's own timeout and token budget.
        // `-` makes the CLI read the prompt from stdin, so nothing of
        // it shows in the process list.
        args.push("-".to_string());
        Ok(args)
    }

    /// The sandbox the session's commands run in, and the configuration
    /// it opens with inside it.
    fn confinement_args(&self, req: &SessionRequest) -> Vec<String> {
        let settings = self.settings.as_ref().cloned().unwrap_or_default();
        let edit_sandbox = settings.sandbox.unwrap_or_default();
        let mut args = fence::sandbox_args(&req.fence, req.permissions, edit_sandbox);
        // Only `workspace-write` reads the switch: `read-only` has no
        // network to open and `danger-full-access` never closed it.
        let opens_network = req.network
            && fence::mode(&req.fence, req.permissions, edit_sandbox)
                == crate::session::Sandbox::WorkspaceWrite;
        args.extend(config_overrides(req, opens_network));
        args
    }

    async fn launch(
        &self,
        req: SessionRequest,
        resume: Option<&SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        // Settings that do not read fail the session rather than fall
        // back: a `sandbox:` nobody could parse would run the agent
        // under a confinement the team never asked for, and silently.
        if let Err(unreadable) = &self.settings {
            return Err(AdapterError::UnreadableSettings {
                adapter: ID.clone(),
                detail: unreadable.to_string(),
            });
        }
        crate::skills::stage(&ID, SKILLS_MOUNT, &req)?;
        let args =
            self.build_args(&req, resume)
                .map_err(|source| AdapterError::FenceUnbuildable {
                    adapter: ID.clone(),
                    source,
                })?;
        // The credential the config names, placed where a secret is
        // allowed to travel: the child's own environment.
        let mut env = req.env.clone();
        if let Some(endpoint) = &req.run_tools_endpoint {
            env.insert(TOKEN_VAR.to_string(), endpoint.token.clone());
        }
        // Where the hooks reach the engine, inherited by every hook this
        // CLI runs.
        if let Some(rail) = &req.rail {
            env.extend(rail.env().map(|(var, value)| (var.to_string(), value)));
        }
        let mode = fence::mode(
            &req.fence,
            req.permissions,
            self.settings
                .as_ref()
                .ok()
                .and_then(|settings| settings.sandbox)
                .unwrap_or_default(),
        );
        subprocess::open(Launch {
            adapter: &ID,
            binary: &self.binary,
            args,
            cwd: &req.cwd,
            env: &env,
            prompt: &req.prompt,
            parser: Box::new(CodexParser {
                last_message: String::new(),
                model: req.model.clone(),
                fence: fence::coverage(&req.fence, &req.cwd, mode),
            }),
        })
        .await
    }
}

/// The `-c` overrides one request needs, in the order they are written.
///
/// What a session may reach beyond its working directory, and how it
/// reaches the run's own tools: both are settings of the CLI's config
/// file, which `-c` overrides for this invocation alone rather than
/// writing to the user's own `~/.codex/config.toml`.
fn config_overrides(req: &SessionRequest, opens_network: bool) -> Vec<String> {
    let mut args = Vec::new();
    // `workspace-write` confines writes to the workspace, and the
    // fence's roots are what sits outside it and stays writable — the
    // directory a declared file belongs in, first of all. Without this
    // the session is told to write a file the sandbox then refuses it.
    let roots = fence::writable_roots(&req.fence);
    if !roots.is_empty() {
        args.extend(
            ConfigOverride::list("sandbox_workspace_write.writable_roots", roots).into_args(),
        );
    }
    // The same sandbox keeps every socket off, loopback included, so a
    // command the session runs cannot even listen on 127.0.0.1.
    if opens_network {
        args.extend(
            ConfigOverride::bool("sandbox_workspace_write.network_access", true).into_args(),
        );
    }
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

/// The CLI's JSONL, one event list per line. The last note seen is
/// what a turn's completion reports as its summary, so it travels from
/// line to line.
struct CodexParser {
    last_message: String,
    /// The model the session was launched with: the CLI's stream names
    /// none, and the session runs on the one it was told.
    model: Option<yunta_core::ModelName>,
    /// What the sandbox this session runs under actually fenced —
    /// computed once, when the session was built.
    fence: Coverage,
}

impl LineParser for CodexParser {
    fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        let mut events = parse::parse_line(line, &self.last_message, &self.fence);
        for event in &mut events {
            if let AgentEvent::SessionOpened { model, .. } = event {
                model.clone_from(&self.model);
            }
        }
        if let Some(text) = events.iter().rev().find_map(|event| match event {
            AgentEvent::Note { text } => Some(text.clone()),
            _ => None,
        }) {
            self.last_message = text;
        }
        events
    }
}

#[async_trait]
impl Adapter for CodexAdapter {
    fn id(&self) -> &'static AdapterId {
        &ID
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resume_session: true,
            // Mounted by staging into the session cwd's own
            // `.agents/skills/` — where the CLI discovers a project's
            // skills.
            skills: true,
            // Every edit goes through a `PreToolUse` hook that asks the
            // engine before the patch applies; commands stay confined by
            // the process's sandbox, which the coverage reports.
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
            // The `read-only` and `workspace-write` sandboxes keep a
            // session off the network unless its request opens it; a
            // `full` session, unconfined on every adapter, is one `check`
            // refuses `network: false` for.
            network_isolation: true,
            // The CLI names an MCP server's tools `mcp__<server>__<tool>`
            // with the server's `-` written `_`: a session finds
            // `mcp__yunta_run__yunta_task`, never `yunta_task`.
            tool_naming: ToolNaming::McpPrefixedUnderscored,
        }
    }

    fn staged_paths(&self, req: &SessionRequest) -> Vec<PathBuf> {
        crate::skills::mounts(SKILLS_MOUNT, req)
            .into_iter()
            .map(|(mount, _)| mount)
            .collect()
    }

    fn unstage(&self, req: &SessionRequest) -> Result<()> {
        crate::skills::unstage(&ID, SKILLS_MOUNT, req)
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
        Ok(subprocess::probe_version(&self.binary).await)
    }

    fn own_config(&self) -> Vec<yunta_core::ScopeGlob> {
        [".codex/config.toml", ".codex/hooks.json"]
            .into_iter()
            .filter_map(|path| yunta_core::ScopeGlob::exact(std::path::Path::new(path)).ok())
            .collect()
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
