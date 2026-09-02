//! The `mock` adapter — a first-class adapter, not a test helper: it
//! reproduces sessions from YAML fixtures (a script of events plus
//! filesystem effects), with injectable failures and latency, so the
//! engine's whole cycle — tasks, degradation, cancellation, resume,
//! eventually parallelism — is testable without an LLM. A fixture
//! scripts every session of a run in spawn order; see [`MockFixture`].

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
    /// claimed it. Replaces a bare atomic counter: concurrent task
    /// dispatch races several `spawn()` calls at once, so "the next
    /// index" stops meaning "the right script" — see
    /// `SessionScript::match_prompt_contains`.
    consumed: Mutex<Vec<bool>>,
    /// Every `spawn()`'s `req.skills`, in claim order: the mock's
    /// "native mount" is recording what it was asked to mount, so
    /// engine tests assert the whole resolution chain without a CLI.
    skills_seen: Mutex<Vec<Vec<std::path::PathBuf>>>,
    /// Every `spawn()`'s `req.agent`, in claim order — same
    /// record-the-mount principle as `skills_seen`.
    agents_seen: Mutex<Vec<Option<String>>>,
    /// Every `resume()`'s session id, in call order: the mock's
    /// "resume" is serving the next script under the SAME session id —
    /// recording which one proves the engine handed back the
    /// conversation it meant to continue.
    resumes_seen: Mutex<Vec<SessionId>>,
    /// Every session's `req.run_tools_endpoint`, in claim order — same
    /// record-the-mount principle as `skills_seen`: engine tests prove
    /// the endpoint reached the session (or deliberately didn't)
    /// without a real CLI.
    endpoints_seen: Mutex<Vec<Option<crate::RunToolsEndpoint>>>,
}

impl MockAdapter {
    pub fn new(fixture: MockFixture) -> Self {
        let consumed = Mutex::new(vec![false; fixture.sessions.len()]);
        Self {
            fixture,
            consumed,
            skills_seen: Mutex::new(Vec::new()),
            agents_seen: Mutex::new(Vec::new()),
            resumes_seen: Mutex::new(Vec::new()),
            endpoints_seen: Mutex::new(Vec::new()),
        }
    }

    /// Every session id `resume()` was asked to continue, in call order.
    pub fn resumes_seen(&self) -> Vec<SessionId> {
        self.resumes_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The `agent` of every session spawned so far, in claim order.
    pub fn agents_seen(&self) -> Vec<Option<String>> {
        self.agents_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The `skills` of every session spawned so far, in claim order.
    pub fn skills_seen(&self) -> Vec<Vec<std::path::PathBuf>> {
        self.skills_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The `run_tools_endpoint` of every session so far, in claim order.
    pub fn endpoints_seen(&self) -> Vec<Option<crate::RunToolsEndpoint>> {
        self.endpoints_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn from_yaml(yaml: &str) -> std::result::Result<Self, yunta_core::yaml::YamlError> {
        Ok(Self::new(yunta_core::yaml::parse(yaml)?))
    }

    /// Whether an effect at `path` is blocked by the request's edit
    /// constraints: only a hook-capable adapter blocks, and only a path
    /// no declared glob matches. No constraints means nothing to block.
    fn is_blocked(&self, req: &SessionRequest, path: &std::path::Path) -> bool {
        self.fixture.capabilities.edit_hooks
            && req.edit_constraints.as_ref().is_some_and(|globs| {
                yunta_core::scope_globset(globs).is_ok_and(|set| !set.is_match(path))
            })
    }

    /// Applies one session's filesystem effects under the request's
    /// `cwd`, leaving out the ones its edit constraints block: a
    /// hook-capable adapter installs the block before the edit ever
    /// lands; without the capability, the engine's own post-check scope
    /// diff is what catches it instead.
    fn apply_effects(&self, script: &SessionScript, req: &SessionRequest) -> Result<()> {
        let cwd = &req.cwd;
        for effect in &script.effects {
            if self.is_blocked(req, &effect.path) {
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
        self.spawn_scripted(req, None)
    }

    /// Serves the next matching script exactly like `spawn`, but under
    /// the session id being resumed — a real adapter continues the
    /// same conversation, so the stream reports the same identity.
    async fn resume(
        &self,
        session: &SessionId,
        req: SessionRequest,
    ) -> Result<Box<dyn AgentSession>> {
        self.resumes_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(session.clone());
        self.spawn_scripted(req, Some(session.clone()))
    }
}

impl MockAdapter {
    fn spawn_scripted(
        &self,
        req: SessionRequest,
        resume_as: Option<SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        self.skills_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req.skills.clone());
        self.endpoints_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req.run_tools_endpoint.clone());
        self.agents_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req.agent.clone());
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
            // prompt at all, in declaration order. This is the whole
            // behavior for every fixture that doesn't use
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

        self.apply_effects(script, &req)?;

        let session_id = resume_as.unwrap_or_else(|| {
            SessionId::from(format!(
                "mock-session-{}",
                SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
            ))
        });

        let blocked_markers: Vec<PathBuf> = script
            .effects
            .iter()
            .filter(|e| self.is_blocked(&req, &e.path))
            .map(|e| e.path.clone())
            .collect();

        let (tx, rx) = mpsc::unbounded_channel();
        let notify = Arc::new(Notify::new());
        let task_notify = Arc::clone(&notify);

        let model = script.model.clone();
        let steps = script.steps.clone();
        let outcome = script.outcome.clone();
        let run_tools_endpoint = req.run_tools_endpoint.clone();

        tokio::spawn(async move {
            // SessionOpened is always the first event, unconditionally.
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
                    fixture::MockStep::RunTool {
                        tool, arguments, ..
                    } => {
                        // A real MCP call over the wire — a tool error
                        // fails the whole session loudly: fixtures
                        // script intent, and an intent the engine
                        // refuses is a test outcome, not noise.
                        match call_run_tool(run_tools_endpoint.as_ref(), &tool, arguments).await {
                            Ok(digest) => AgentEvent::ToolUse {
                                name: tool,
                                target_digest: digest,
                            },
                            Err(message) => {
                                let _ = tx.send(AgentEvent::Failed {
                                    error: AgentError { message },
                                    retryable: false,
                                });
                                return;
                            }
                        }
                    }
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
                // Both end with no terminal event — a real crash: the
                // engine synthesizes Failed{retryable:true}, not the
                // adapter. Hang additionally waits for interrupt/kill
                // before ending, simulating a stuck session a timeout
                // would have to act on.
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

/// The mock's own MCP client leg: one `tools/call` against the
/// session's per-run endpoint, exactly as a real CLI would place it.
/// Returns a short digest of the response for the audit stream
/// (`ToolUse.target_digest` — never full content), or the error text
/// that fails the session.
async fn call_run_tool(
    endpoint: Option<&crate::RunToolsEndpoint>,
    tool: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> std::result::Result<String, String> {
    use rmcp::ServiceExt;

    let Some(endpoint) = endpoint else {
        return Err(format!(
            "fixture step run_tool `{tool}` but this session has no run_tools_endpoint — \
             the engine never offered one (missing `run_tools` capability, or the \
             listener wasn't opened)"
        ));
    };
    let config =
        rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
            endpoint.url.clone(),
        )
        .auth_header(endpoint.token.expose().clone());
    let transport = rmcp::transport::StreamableHttpClientTransport::with_client(
        reqwest::Client::default(),
        config,
    );
    let client = ()
        .serve(transport)
        .await
        .map_err(|e| format!("run_tool `{tool}`: cannot reach the per-run endpoint: {e}"))?;
    let mut params = rmcp::model::CallToolRequestParams::new(tool.to_string());
    if !arguments.is_empty() {
        params = params.with_arguments(arguments);
    }
    let result = client.call_tool(params).await;
    let _ = client.cancel().await;
    let result = result.map_err(|e| format!("run_tool `{tool}` failed: {e}"))?;
    let text: String = result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    if result.is_error.unwrap_or(false) {
        return Err(format!("run_tool `{tool}` returned an error: {text}"));
    }
    let hash = yunta_core::sha256_hex(text.as_bytes());
    Ok(format!("{tool}:{}", &hash[..12]))
}
