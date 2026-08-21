//! The real `claude-code` adapter: spawns the `claude` CLI headless
//! (`-p --output-format stream-json`), streams its JSON lines into
//! `AgentEvent`s (`parse.rs`), and maps yunta's portable request fields
//! onto the CLI's own flags (`permissions.rs`). Never exercised by the
//! automated suite — no real LLM in CI — covered instead by
//! `crates/adapters/tests/claude_code.rs` against a scripted fake binary,
//! plus one manual smoke test.

mod parse;
mod permissions;

use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use yunta_core::{AdapterSettings, Capabilities, Result, SessionId, YuntaError};

use crate::session::{Adapter, AgentEvent, AgentSession, ProbeReport, SessionRequest};

pub struct ClaudeCodeAdapter {
    binary: PathBuf,
}

impl ClaudeCodeAdapter {
    pub fn new(settings: &AdapterSettings) -> Self {
        Self {
            binary: settings
                .binary
                .clone()
                .unwrap_or_else(|| PathBuf::from("claude")),
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
            args.push(model.clone());
        }
        // Only populated if capabilities().custom_agents — true here.
        if let Some(agent) = &req.agent {
            args.push("--agent".to_string());
            args.push(agent.clone());
        }
        args.extend(permissions::permission_args(req.permissions));
        args.push(req.prompt.clone());
        args
    }

    async fn open_session(
        &self,
        req: SessionRequest,
        resume: Option<&SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        stage_skills(&req)?;
        let args = self.build_args(&req, resume);

        let mut std_cmd = std::process::Command::new(&self.binary);
        std_cmd
            .args(&args)
            .current_dir(&req.cwd)
            .envs(&req.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // The whole session's process tree must die together on
        // interrupt/kill. Putting the child in its own process group
        // means a group-targeted signal (negative pid) reaches every
        // descendant the CLI spawns — its own tool subprocesses included
        // — not just the CLI process itself.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            std_cmd.process_group(0);
        }

        let mut child = tokio::process::Command::from(std_cmd)
            .spawn()
            .map_err(|source| YuntaError::AdapterIo {
                adapter: "claude-code".to_string(),
                action: "spawn the claude subprocess".to_string(),
                source,
            })?;

        let pid = child.id().ok_or_else(|| YuntaError::Adapter {
            adapter: "claude-code".to_string(),
            message: "the claude subprocess exited before it could be tracked".to_string(),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| YuntaError::Adapter {
            adapter: "claude-code".to_string(),
            message: "the claude subprocess has no stdout pipe".to_string(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| YuntaError::Adapter {
            adapter: "claude-code".to_string(),
            message: "the claude subprocess has no stderr pipe".to_string(),
        })?;

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        for event in parse::parse_line(&line) {
                            if tx.send(event).is_err() {
                                return;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(error = %e, "claude-code: error reading stdout");
                        break;
                    }
                }
            }
            // Reap the child so a killed or naturally-finished session
            // never leaves a zombie behind.
            let _ = child.wait().await;
        });
        tokio::spawn(drain_stderr(stderr));

        Ok(Box::new(ClaudeCodeSession {
            pid,
            receiver: Some(rx),
        }))
    }
}

async fn drain_stderr(stderr: tokio::process::ChildStderr) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::debug!(target: "claude_code_stderr", "{line}");
    }
}

#[async_trait]
impl Adapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude-code"
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
        let output = tokio::process::Command::new(&self.binary)
            .arg("--version")
            .output()
            .await;
        Ok(match output {
            Ok(output) if output.status.success() => ProbeReport {
                healthy: true,
                version: Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
                diagnostic: None,
            },
            Ok(output) => ProbeReport {
                healthy: false,
                version: None,
                diagnostic: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            },
            Err(e) => ProbeReport {
                healthy: false,
                version: None,
                diagnostic: Some(e.to_string()),
            },
        })
    }

    async fn spawn(&self, req: SessionRequest) -> Result<Box<dyn AgentSession>> {
        self.open_session(req, None).await
    }

    async fn resume(
        &self,
        session: &SessionId,
        req: SessionRequest,
    ) -> Result<Box<dyn AgentSession>> {
        self.open_session(req, Some(session)).await
    }
}

pub struct ClaudeCodeSession {
    pid: u32,
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
}

#[async_trait]
impl AgentSession for ClaudeCodeSession {
    fn events(&mut self) -> BoxStream<'_, AgentEvent> {
        match self.receiver.take() {
            Some(rx) => Box::pin(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })),
            // A second call gets an already-exhausted stream rather than
            // a panic — the caller's misuse, not a reason to crash.
            None => Box::pin(stream::empty()),
        }
    }

    async fn interrupt(&mut self) -> Result<()> {
        signal_group(self.pid, "-INT").await
    }

    async fn kill(&mut self) -> Result<()> {
        signal_group(self.pid, "-KILL").await
    }

    fn pgid(&self) -> Option<u32> {
        // Spawned with `process_group(0)`, so the child's pid is its
        // process-group id.
        Some(self.pid)
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
    let io_err = |action: String, source: std::io::Error| YuntaError::AdapterIo {
        adapter: "claude-code".to_string(),
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

/// Sends `signal` to the whole process group — a negative pid
/// targets every descendant the CLI spawned, not just the CLI process
/// itself. The `--` before the negative pid is load-bearing: procps-ng's
/// `kill` (confirmed empirically) silently signals nothing and still
/// exits 0 without it, parsing `-KILL -123` as two flags instead of a
/// signal plus a process-group target. `kill` exiting nonzero because
/// the group is already gone is the desired end state, not a failure
/// worth reporting.
async fn signal_group(pid: u32, signal: &str) -> Result<()> {
    let _ = tokio::process::Command::new("kill")
        .arg(signal)
        .arg("--")
        .arg(format!("-{pid}"))
        .status()
        .await
        .map_err(|source| YuntaError::AdapterIo {
            adapter: "claude-code".to_string(),
            action: format!("send {signal} to the session's process group"),
            source,
        })?;
    Ok(())
}
