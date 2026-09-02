//! The real `claude-code` adapter: spawns the `claude` CLI headless
//! (`-p --output-format stream-json`), streams its JSON lines into
//! `AgentEvent`s (`parse.rs`), and maps yunta's portable request fields
//! onto the CLI's own flags (`permissions.rs`). Never exercised by the
//! automated suite — no real LLM in CI — covered instead by
//! `crates/adapters/tests/claude_code.rs` against a scripted fake binary,
//! plus one manual smoke test.

mod parse;
mod permissions;
mod settings;

use std::path::PathBuf;

use async_trait::async_trait;
use yunta_core::{AdapterError, AdapterId, AdapterSettings, Capabilities, Result, SessionId};

use crate::session::{Adapter, AgentEvent, AgentSession, ProbeReport, SessionRequest};
use crate::subprocess::{self, Launch, LineParser};

/// The id config names this adapter by.
pub static ID: AdapterId = AdapterId::from_static("claude-code");

pub struct ClaudeCodeAdapter {
    binary: PathBuf,
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
            settings: settings::ClaudeSettings::read(settings),
        }
    }

    fn build_args(&self, req: &SessionRequest, resume: Option<&SessionId>) -> Vec<String> {
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
        // Only populated if capabilities().custom_agents — true here.
        if let Some(agent) = &req.agent {
            args.push("--agent".to_string());
            args.push(agent.to_string());
        }
        args.extend(permissions::permission_args(req.permissions));
        if let Some(max_turns) = req.budget.max_turns {
            args.push("--max-turns".to_string());
            args.push(max_turns.to_string());
        }
        // The prompt arrives on stdin: `-p` with no positional prompt
        // reads it there, and nothing of it shows in the process list.
        args
    }

    async fn launch(
        &self,
        req: SessionRequest,
        resume: Option<&SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        stage_skills(&req)?;
        let args = self.build_args(&req, resume);
        subprocess::open(Launch {
            adapter: &ID,
            binary: &self.binary,
            args,
            cwd: &req.cwd,
            env: &req.env,
            prompt: &req.prompt,
            parser: Box::new(ClaudeParser),
        })
        .await
    }
}

/// The CLI's stream-json lines, one event list per line.
struct ClaudeParser;

impl LineParser for ClaudeParser {
    fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        parse::parse_line(line)
    }
}

#[async_trait]
impl Adapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static AdapterId {
        &ID
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resume_session: true,
            // No live edit-hook blocking wired for the real CLI — a
            // capability must never claim more than is actually built,
            // so this stays false. The engine's own post-hoc scope
            // check is the real boundary today.
            edit_hooks: false,
            permission_profiles: true,
            custom_agents: true,
            usage_reporting: true,
            // MCP per-run tools aren't wired yet.
            run_tools: false,
            // Mounted by staging into the session cwd's own
            // `.claude/skills/` — the CLI's native discovery location.
            skills: true,
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

/// The CLI's native skills discovery is `.claude/skills/` under
/// its working directory — mounting is staging a symlink per resolved
/// skill directory there, named after the directory itself. Re-staging
/// (a retry, a resume) replaces the link; the engine's scope check
/// ignores this engine-staged path, so it never reads as agent work.
fn stage_skills(req: &SessionRequest) -> Result<()> {
    if req.skills.is_empty() {
        return Ok(());
    }
    let io_err = |action: String, source: std::io::Error| AdapterError::AdapterIo {
        adapter: ID.clone(),
        action,
        source,
    };
    let skills_root = req.cwd.join(".claude").join("skills");
    std::fs::create_dir_all(&skills_root)
        .map_err(|e| io_err(format!("create {}", skills_root.display()), e))?;
    for skill in &req.skills {
        let Some(name) = skill.file_name() else {
            continue;
        };
        let dest = skills_root.join(name);
        match std::fs::remove_file(&dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(format!("replace {}", dest.display()), e)),
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(skill, &dest)
            .map_err(|e| io_err(format!("stage skill at {}", dest.display()), e))?;
    }
    Ok(())
}
