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
mod settings;

use std::path::PathBuf;

use async_trait::async_trait;
use yunta_core::{
    AdapterError, AdapterId, AdapterSettings, Capabilities, FenceLevel, Result, SessionId,
    Unbuildable,
};

use yunta_core::fence::Coverage;
use yunta_core::port::{
    Adapter, AgentEvent, AgentSession, ProbeReport, RunToolsEndpoint, SessionRequest,
};
use yunta_core::process::subprocess::{self, Launch, LineParser};

use config::ConfigOverride;

/// The id config names this adapter by.
pub static ID: AdapterId = AdapterId::from_static("codex");

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
        // the fallback here is a node whose settings named no sandbox.
        let edit_sandbox = self
            .settings
            .as_ref()
            .ok()
            .and_then(|settings| settings.sandbox)
            .unwrap_or_default();
        args.extend(fence::sandbox_args(
            &req.fence,
            req.permissions,
            edit_sandbox,
        )?);
        args.extend(config_overrides(req));
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
        subprocess::open(Launch {
            adapter: &ID,
            binary: &self.binary,
            args,
            cwd: &req.cwd,
            env: &env,
            prompt: &req.prompt,
            parser: Box::new(CodexParser {
                last_message: String::new(),
                fence: fence::coverage(&req.fence, &req.cwd),
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
fn config_overrides(req: &SessionRequest) -> Vec<String> {
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
    if let Some(endpoint) = &req.run_tools_endpoint {
        let server = RunToolsEndpoint::SERVER_NAME;
        // The per-run server reaches the CLI as an external MCP server
        // over streamable HTTP: `url` is the key that selects that
        // transport, and the credential travels as the name of the
        // variable the CLI reads it from.
        for setting in [
            ConfigOverride::string(format!("mcp_servers.{server}.url"), &endpoint.url),
            ConfigOverride::string(
                format!("mcp_servers.{server}.bearer_token_env_var"),
                TOKEN_VAR,
            ),
            ConfigOverride::string(
                format!("mcp_servers.{server}.default_tools_approval_mode"),
                "approve",
            ),
        ] {
            args.extend(setting.into_args());
        }
    }
    args
}

/// The CLI's JSONL, one event list per line. The last note seen is
/// what a turn's completion reports as its summary, so it travels from
/// line to line.
struct CodexParser {
    last_message: String,
    /// What the sandbox this session runs under actually fenced —
    /// computed once, when the session was built.
    fence: Coverage,
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

#[async_trait]
impl Adapter for CodexAdapter {
    fn id(&self) -> &'static AdapterId {
        &ID
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resume_session: true,
            // `codex exec` has no skills mechanism to mount into — a
            // capability must never claim more than is actually built —
            // so the engine degrades with `capability_degraded` when a
            // node declares skills here.
            skills: false,
            // The sandbox the process itself runs under keeps writes
            // inside a set of directories — by directory, never by glob,
            // which is what the coverage a session reports says.
            fence: FenceLevel::Filesystem,
            permission_profiles: true,
            // `codex exec` has no documented `--agent <name>` selector —
            // nothing here to map `agent:` onto, so declaring the
            // capability would be claiming something that isn't built.
            custom_agents: false,
            usage_reporting: true,
            // The per-run MCP server reaches the CLI as an external
            // streamable-HTTP server, configured by `-c` overrides.
            // Like every other mapping in this adapter, this is read
            // off the CLI's own source rather than confirmed live —
            // see this module's doc comment.
            run_tools: true,
            // `codex exec` isolates no network — declaring the capability
            // would claim a sandbox that isn't built, so `network: false`
            // degrades to declarative-only here.
            network_isolation: false,
        }
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
