//! The real `codex` adapter: spawns `codex exec --json` headless,
//! streams its JSONL into `AgentEvent`s (`parse.rs`), and maps yunta's
//! portable request fields onto the CLI's own flags (`permissions.rs`).
//! Never exercised by the automated suite — no real LLM in CI — covered
//! instead by `crates/adapters/tests/codex.rs` against a scripted fake
//! binary, matching `claude_code`'s own testing shape exactly.
//!
//! **No live smoke test against the real binary — a documented gap, not
//! a silent skip.** The `claude_code` adapter's own manual smoke test
//! ran against `claude`, which is installed and authenticated in that
//! sandbox. No `codex` binary exists here (`which codex` finds nothing)
//! and no OpenAI credentials are configured — there is no way to run one
//! from this environment. What the wire protocol looks like isn't a
//! guess, though: `parse.rs`'s own doc comment cites the CLI's own
//! source (`codex-rs/exec/src/exec_events.rs`, openai/codex) for every
//! event and field shape this adapter reads, the same rigor applied to
//! `claude_code`'s own CLI-specific mapping — the piece that's missing
//! is only live confirmation that the installed binary actually behaves
//! the way its own source says it should.
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

mod parse;
mod permissions;
mod settings;

use std::path::PathBuf;

use async_trait::async_trait;
use yunta_core::{
    AdapterError, AdapterId, AdapterSettings, Capabilities, ModelName, Result, SessionId,
};

use crate::session::{Adapter, AgentEvent, AgentSession, ProbeReport, SessionRequest};
use crate::subprocess::{self, Launch, LineParser};

/// The id config names this adapter by.
pub static ID: AdapterId = AdapterId::from_static("codex");

/// What `SessionOpened.model` reports for a request that names no
/// model: the CLI picks its own and never says which.
static DEFAULT_MODEL: ModelName = ModelName::from_static("default");

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

    fn build_args(&self, req: &SessionRequest, resume: Option<&SessionId>) -> Vec<String> {
        let mut args = vec!["exec".to_string(), "--json".to_string()];
        if let Some(session_id) = resume {
            args.push("resume".to_string());
            args.push(session_id.as_str().to_string());
        }
        if let Some(model) = &req.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        let edit_sandbox = self
            .settings
            .as_ref()
            .ok()
            .and_then(|settings| settings.sandbox)
            .unwrap_or_default();
        args.extend(permissions::sandbox_args(req.permissions, edit_sandbox));
        // `codex exec` exposes no cap on turns: `budget.max_turns` is
        // bounded here by the engine's own timeout and token budget.
        // `-` makes the CLI read the prompt from stdin, so nothing of
        // it shows in the process list.
        args.push("-".to_string());
        args
    }

    async fn launch(
        &self,
        req: SessionRequest,
        resume: Option<&SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        let requested_model = req.model.clone().unwrap_or_else(|| DEFAULT_MODEL.clone());
        let args = self.build_args(&req, resume);
        subprocess::open(Launch {
            adapter: &ID,
            binary: &self.binary,
            args,
            cwd: &req.cwd,
            env: &req.env,
            prompt: &req.prompt,
            parser: Box::new(CodexParser {
                requested_model,
                last_message: String::new(),
            }),
        })
        .await
    }
}

/// The CLI's JSONL, one event list per line. The last note seen is
/// what a turn's completion reports as its summary, so it travels from
/// line to line.
struct CodexParser {
    requested_model: ModelName,
    last_message: String,
}

impl LineParser for CodexParser {
    fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        let events = parse::parse_line(line, &self.requested_model, &self.last_message);
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
            // No live edit-hook blocking wired — same honest gap as
            // claude_code, same reason: the engine's own post-hoc scope
            // check is the real boundary today.
            edit_hooks: false,
            permission_profiles: true,
            // `codex exec` has no documented `--agent <name>` selector —
            // nothing here to map `agent:` onto, so declaring the
            // capability would be claiming something that isn't built.
            custom_agents: false,
            usage_reporting: true,
            // MCP per-run tools aren't wired yet.
            run_tools: false,
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
