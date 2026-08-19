//! The `mock` adapter (T3.2) — a first-class adapter, not a test helper
//! (Spec Adapter §6): it reproduces sessions from YAML fixtures (a script
//! of events plus filesystem effects), with injectable failures and
//! latency, so the engine's whole cycle — tasks, degradation,
//! cancellation, resume, eventually paralelismo — is testable without an
//! LLM (A8). A fixture scripts every session of a run in spawn order;
//! see [`MockFixture`].

mod fixture;

pub use fixture::{MockEffect, MockFixture, MockOutcome, MockStep, SessionScript};

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::sync::{mpsc, Notify};
use yunta_core::{Capabilities, Result, SessionId, YuntaError};

use crate::session::{
    Adapter, AgentError, AgentEvent, AgentOutcome, AgentSession, ProbeReport, SessionRequest,
};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct MockAdapter {
    fixture: MockFixture,
    /// One flag per `fixture.sessions` entry — `true` once `spawn()` has
    /// claimed it. Replaces a bare atomic counter (T5.10: concurrent task
    /// dispatch races several `spawn()` calls at once, so "the next
    /// index" stops meaning "the right script" — see
    /// `SessionScript::match_prompt_contains`).
    consumed: Mutex<Vec<bool>>,
}

impl MockAdapter {
    pub fn new(fixture: MockFixture) -> Self {
        let consumed = Mutex::new(vec![false; fixture.sessions.len()]);
        Self { fixture, consumed }
    }

    pub fn from_yaml(yaml: &str) -> std::result::Result<Self, serde_yaml::Error> {
        Ok(Self::new(serde_yaml::from_str(yaml)?))
    }

    /// Applies one session's filesystem effects under `cwd`, honoring
    /// `blocked` + `edit_hooks` (O5: a hook-capable adapter installs the
    /// block before the edit ever lands; without the capability, the
    /// engine's own post-check scope diff — T5.3 — is what catches it
    /// instead).
    fn apply_effects(&self, script: &SessionScript, cwd: &std::path::Path) -> Result<()> {
        for effect in &script.effects {
            if effect.blocked && self.fixture.capabilities.edit_hooks {
                continue;
            }
            let full_path = cwd.join(&effect.path);
            let io_err = |action: String, source: std::io::Error| YuntaError::AdapterIo {
                adapter: "mock".to_string(),
                action,
                source,
            };
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    io_err(format!("create directories for {}", full_path.display()), e)
                })?;
            }
            std::fs::write(&full_path, &effect.content)
                .map_err(|e| io_err(format!("write {}", full_path.display()), e))?;
        }
        Ok(())
    }
}

#[async_trait]
impl Adapter for MockAdapter {
    fn id(&self) -> &'static str {
        "mock"
    }

    fn capabilities(&self) -> Capabilities {
        self.fixture.capabilities
    }

    async fn probe(&self) -> Result<ProbeReport> {
        Ok(ProbeReport {
            healthy: true,
            version: Some("mock-0.1".to_string()),
            diagnostic: None,
        })
    }

    async fn spawn(&self, req: SessionRequest) -> Result<Box<dyn AgentSession>> {
        let index = {
            let mut consumed = self.consumed.lock().unwrap_or_else(|e| e.into_inner());
            let claim = self
                .fixture
                .sessions
                .iter()
                .enumerate()
                .find_map(|(i, script)| {
                    let matches = script
                        .match_prompt_contains
                        .as_deref()
                        .is_some_and(|needle| req.prompt.contains(needle));
                    (!consumed[i] && matches).then_some(i)
                });
            // No script named this request explicitly — fall back to the
            // next unconsumed script that never opted into matching by
            // prompt at all, in declaration order. This is the entire
            // pre-T5.10 behavior for every fixture that doesn't use
            // `match_prompt_contains`.
            let claim = claim.or_else(|| {
                self.fixture
                    .sessions
                    .iter()
                    .enumerate()
                    .find(|(i, script)| !consumed[*i] && script.match_prompt_contains.is_none())
                    .map(|(i, _)| i)
            });
            let Some(index) = claim else {
                return Err(YuntaError::Adapter {
                    adapter: "mock".to_string(),
                    message: format!(
                        "fixture exhausted: {} scripted session(s), none left unconsumed and \
                         matching this request — add a session to the fixture for every \
                         session the run opens",
                        self.fixture.sessions.len(),
                    ),
                });
            };
            consumed[index] = true;
            index
        };
        let script = &self.fixture.sessions[index];

        self.apply_effects(script, &req.cwd)?;

        let session_id = SessionId::from(format!(
            "mock-session-{}",
            SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));

        let blocked_markers: Vec<PathBuf> = script
            .effects
            .iter()
            .filter(|e| e.blocked && self.fixture.capabilities.edit_hooks)
            .map(|e| e.path.clone())
            .collect();

        let (tx, rx) = mpsc::unbounded_channel();
        let notify = Arc::new(Notify::new());
        let task_notify = Arc::clone(&notify);

        let model = script.model.clone();
        let steps = script.steps.clone();
        let outcome = script.outcome.clone();

        tokio::spawn(async move {
            // O1: SessionOpened is always the first event, unconditionally.
            if tx
                .send(AgentEvent::SessionOpened { session_id, model })
                .is_err()
            {
                return;
            }

            for path in blocked_markers {
                if tx
                    .send(AgentEvent::ToolUse {
                        name: "edit".to_string(),
                        target_digest: format!("blocked:{}", path.display()),
                    })
                    .is_err()
                {
                    return;
                }
            }

            for step in steps {
                let delay = Duration::from_millis(step.after_ms());
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = task_notify.notified() => return, // interrupted/killed mid-stream
                }
                let event = match step {
                    fixture::MockStep::ToolUse {
                        name,
                        target_digest,
                        ..
                    } => AgentEvent::ToolUse {
                        name,
                        target_digest,
                    },
                    fixture::MockStep::Usage {
                        input_tokens,
                        output_tokens,
                        cached_input_tokens,
                        ..
                    } => AgentEvent::Usage {
                        input_tokens,
                        output_tokens,
                        cached_input_tokens,
                    },
                    fixture::MockStep::Note { text, .. } => AgentEvent::Note { text },
                };
                if tx.send(event).is_err() {
                    return;
                }
            }

            match outcome {
                MockOutcome::Completed { summary } => {
                    let _ = tx.send(AgentEvent::Completed {
                        result: AgentOutcome { summary },
                    });
                }
                MockOutcome::Failed { message, retryable } => {
                    let _ = tx.send(AgentEvent::Failed {
                        error: AgentError { message },
                        retryable,
                    });
                }
                // Both end with no terminal event — a real crash (O2: the
                // engine synthesizes Failed{retryable:true}, not the
                // adapter). Hang additionally waits for interrupt/kill
                // before ending, simulating a stuck session a timeout
                // (T3.3) would have to act on.
                MockOutcome::Crash => {}
                MockOutcome::Hang => task_notify.notified().await,
            }
        });

        Ok(Box::new(MockSession {
            receiver: Some(rx),
            notify,
        }))
    }
}

pub struct MockSession {
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    notify: Arc<Notify>,
}

#[async_trait]
impl AgentSession for MockSession {
    fn events(&mut self) -> BoxStream<'_, AgentEvent> {
        match self.receiver.take() {
            Some(rx) => Box::pin(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })),
            // A second call gets an already-exhausted stream rather than a
            // panic — the caller's misuse, not a reason to crash the run.
            None => Box::pin(stream::empty()),
        }
    }

    async fn interrupt(&mut self) -> Result<()> {
        self.notify.notify_one();
        Ok(())
    }

    async fn kill(&mut self) -> Result<()> {
        self.notify.notify_one();
        Ok(())
    }
}
