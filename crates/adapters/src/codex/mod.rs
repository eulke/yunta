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

use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use yunta_core::{AdapterSettings, Capabilities, Result, SessionId, YuntaError};

use crate::session::{Adapter, AgentEvent, AgentSession, ProbeReport, SessionRequest};

pub struct CodexAdapter {
    binary: PathBuf,
}

impl CodexAdapter {
    pub fn new(settings: &AdapterSettings) -> Self {
        Self {
            binary: settings
                .binary
                .clone()
                .unwrap_or_else(|| PathBuf::from("codex")),
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
            args.push(model.clone());
        }
        args.extend(permissions::sandbox_args(req.permissions));
        args.push(req.prompt.clone());
        args
    }

    async fn open_session(
        &self,
        req: SessionRequest,
        resume: Option<&SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        // Codex's own `thread.started` line never carries the model it
        // used (confirmed gap in the CLI — openai/codex#14736, still
        // open) — the request's own model is the only source `parse.rs`
        // has for `SessionOpened.model`, which must always report one.
        let requested_model = req.model.clone().unwrap_or_else(|| "default".to_string());
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
        // interrupt/kill — same reasoning as claude_code's own.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            std_cmd.process_group(0);
        }

        let mut child = tokio::process::Command::from(std_cmd)
            .spawn()
            .map_err(|source| YuntaError::AdapterIo {
                adapter: "codex".to_string(),
                action: "spawn the codex subprocess".to_string(),
                source,
            })?;

        let pid = child.id().ok_or_else(|| YuntaError::Adapter {
            adapter: "codex".to_string(),
            message: "the codex subprocess exited before it could be tracked".to_string(),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| YuntaError::Adapter {
            adapter: "codex".to_string(),
            message: "the codex subprocess has no stdout pipe".to_string(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| YuntaError::Adapter {
            adapter: "codex".to_string(),
            message: "the codex subprocess has no stderr pipe".to_string(),
        })?;

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            // Purely a parsing need: `turn.completed`
            // carries no text of its own (`parse.rs`'s own doc comment)
            // — the last `agent_message` item seen is what becomes the
            // outcome summary when the turn closes.
            let mut last_message = String::new();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        for event in parse::parse_line(&line, &requested_model, &last_message) {
                            if let AgentEvent::Note { text } = &event {
                                last_message = text.clone();
                            }
                            if tx.send(event).is_err() {
                                return;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(error = %e, "codex: error reading stdout");
                        break;
                    }
                }
            }
            let _ = child.wait().await;
        });
        tokio::spawn(drain_stderr(stderr));

        Ok(Box::new(CodexSession {
            pid,
            receiver: Some(rx),
        }))
    }
}

async fn drain_stderr(stderr: tokio::process::ChildStderr) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::debug!(target: "codex_stderr", "{line}");
    }
}

#[async_trait]
impl Adapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
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

pub struct CodexSession {
    pid: u32,
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
}

#[async_trait]
impl AgentSession for CodexSession {
    fn events(&mut self) -> BoxStream<'_, AgentEvent> {
        match self.receiver.take() {
            Some(rx) => Box::pin(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })),
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

/// Sends `signal` to the whole process group — identical mechanism
/// to `claude_code`'s own, see that module's doc comment for why the
/// `--` before the negative pid is load-bearing.
async fn signal_group(pid: u32, signal: &str) -> Result<()> {
    let _ = tokio::process::Command::new("kill")
        .arg(signal)
        .arg("--")
        .arg(format!("-{pid}"))
        .status()
        .await
        .map_err(|source| YuntaError::AdapterIo {
            adapter: "codex".to_string(),
            action: format!("send {signal} to the session's process group"),
            source,
        })?;
    Ok(())
}
