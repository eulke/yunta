//! The `mock` adapter — a first-class adapter, not a test helper: it
//! reproduces sessions from YAML fixtures (a script of events plus
//! filesystem effects), with injectable failures and latency, so the
//! engine's whole cycle — tasks, degradation, cancellation, resume,
//! eventually parallelism — is testable without an LLM. A fixture
//! scripts every session of a run in spawn order; see [`MockFixture`].
//!
//! Spawning a session is claiming a script and handing it to
//! [`script::play`], which owns everything after that: the adapter
//! decides *which* script a request gets and what the session is born
//! with, the player decides what the session then does. What the adapter
//! keeps is the record of what it was asked to mount, which is how an
//! engine test proves a skill, an agent or a run-tools endpoint reached
//! the session without a real CLI.

mod fixture;
mod run_tool;
mod script;

pub use fixture::{
    MockEffect, MockFixture, MockOutcome, MockStep, OnInterrupt, SessionScript, ToolExpectation,
};

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::sync::{mpsc, Notify};
use yunta_core::{AdapterError, AdapterId, AgentName, Capabilities, Result, SessionId};

use crate::session::{Adapter, AgentEvent, AgentSession, ProbeReport, SessionRequest};

/// The id config names this adapter by.
pub static ID: AdapterId = AdapterId::from_static("mock");

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
    agents_seen: Mutex<Vec<Option<AgentName>>>,
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
    /// Every session's `req.artifact_dir`, in claim order — same
    /// record-the-mount principle as `skills_seen`: engine tests prove
    /// which sessions were granted the run's artifact directory without
    /// a real CLI.
    artifact_dirs_seen: Mutex<Vec<Option<std::path::PathBuf>>>,
    /// The next session id's number: every adapter counts from one, so
    /// a fixture's ids never depend on what else ran in the process.
    next_session: AtomicU64,
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
            artifact_dirs_seen: Mutex::new(Vec::new()),
            next_session: AtomicU64::new(1),
        }
    }

    /// The scripts no `spawn()` claimed, by index in the fixture — what
    /// a test asserts is empty once a run opened every session it
    /// scripted.
    pub fn unconsumed(&self) -> Vec<usize> {
        read(&self.consumed)
            .into_iter()
            .enumerate()
            .filter(|(_, claimed)| !claimed)
            .map(|(index, _)| index)
            .collect()
    }

    /// Every session id `resume()` was asked to continue, in call order.
    pub fn resumes_seen(&self) -> Vec<SessionId> {
        read(&self.resumes_seen)
    }

    /// The `agent` of every session spawned so far, in claim order.
    pub fn agents_seen(&self) -> Vec<Option<AgentName>> {
        read(&self.agents_seen)
    }

    /// The `skills` of every session spawned so far, in claim order.
    pub fn skills_seen(&self) -> Vec<Vec<std::path::PathBuf>> {
        read(&self.skills_seen)
    }

    /// The `run_tools_endpoint` of every session so far, in claim order.
    pub fn endpoints_seen(&self) -> Vec<Option<crate::RunToolsEndpoint>> {
        read(&self.endpoints_seen)
    }

    /// The `artifact_dir` of every session so far, in claim order — what a
    /// test reads to see which sessions were granted the run's artifact
    /// directory and which never needed it.
    pub fn artifact_dirs_seen(&self) -> Vec<Option<std::path::PathBuf>> {
        read(&self.artifact_dirs_seen)
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
            let io_err = |action: String, source: std::io::Error| AdapterError::AdapterIo {
                adapter: ID.clone(),
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
    fn id(&self) -> &'static AdapterId {
        &ID
    }

    fn capabilities(&self) -> Capabilities {
        self.fixture.capabilities
    }

    async fn probe(&self) -> Result<ProbeReport> {
        Ok(ProbeReport::Healthy {
            version: Some("mock-0.1".to_string()),
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
        record(&self.resumes_seen, session.clone());
        self.spawn_scripted(req, Some(session.clone()))
    }
}

impl MockAdapter {
    /// Claims the script this request gets and opens the session that
    /// plays it.
    fn spawn_scripted(
        &self,
        req: SessionRequest,
        resume_as: Option<SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        self.record_mount(&req);
        let index = self.claim(&req)?;
        let Some(script) = self.fixture.sessions.get(index) else {
            return Err(AdapterError::Adapter {
                adapter: ID.clone(),
                message: "internal: claimed a session index the fixture does not hold".to_string(),
            });
        };

        self.apply_effects(script, &req)?;

        let (events, receiver) = mpsc::unbounded_channel();
        let interrupt = Arc::new(Notify::new());
        let kill = Arc::new(Notify::new());
        let stops = script::Stops::of(&script.outcome, Arc::clone(&interrupt), Arc::clone(&kill));
        let played = script::Script {
            session_id: self.session_id(resume_as)?,
            model: script.model.clone(),
            blocked_markers: self.blocked_markers(script, &req),
            steps: script.steps.clone(),
            outcome: script.outcome.clone(),
            run_tools_endpoint: req.run_tools_endpoint.clone(),
        };
        tokio::spawn(script::play(played, events, stops));

        Ok(Box::new(MockSession {
            receiver: Some(receiver),
            interrupt,
            kill,
        }))
    }

    /// Records what this session was asked to mount, in claim order.
    fn record_mount(&self, req: &SessionRequest) {
        record(&self.skills_seen, req.skills.clone());
        record(&self.endpoints_seen, req.run_tools_endpoint.clone());
        record(&self.artifact_dirs_seen, req.artifact_dir.clone());
        record(&self.agents_seen, req.agent.clone());
    }

    /// The index of the script this request claims, marked consumed so no
    /// other session gets it.
    fn claim(&self, req: &SessionRequest) -> Result<usize> {
        let mut consumed = self.consumed.lock().unwrap_or_else(|e| e.into_inner());
        let index = match self.named(&consumed, &req.prompt)? {
            Some(index) => index,
            None => self
                .unnamed(&consumed)
                .ok_or_else(|| AdapterError::Adapter {
                    adapter: ID.clone(),
                    message: format!(
                        "fixture exhausted: {} scripted session(s), none left unconsumed and \
                     matching this request — add a session to the fixture for every \
                     session the run opens",
                        self.fixture.sessions.len(),
                    ),
                })?,
        };
        if let Some(slot) = consumed.get_mut(index) {
            *slot = true;
        }
        Ok(index)
    }

    /// The unconsumed script that named this request by a substring of
    /// its prompt, if one did.
    ///
    /// Several scripts with one and the same needle are one request's
    /// attempts, served in declaration order. Scripts with different
    /// needles that both match cannot say which one the request gets:
    /// that fixture is wrong, not lucky.
    fn named(&self, consumed: &[bool], prompt: &str) -> Result<Option<usize>> {
        let matching: Vec<(usize, &str)> = self
            .fixture
            .sessions
            .iter()
            .enumerate()
            .filter(|(i, _)| consumed.get(*i) != Some(&true))
            .filter_map(|(i, script)| {
                script
                    .match_prompt_contains
                    .as_deref()
                    .filter(|needle| prompt.contains(needle))
                    .map(|needle| (i, needle))
            })
            .collect();
        let mut needles: Vec<&str> = matching.iter().map(|(_, needle)| *needle).collect();
        needles.dedup();
        if needles.len() > 1 {
            return Err(AdapterError::Adapter {
                adapter: ID.clone(),
                message: format!(
                    "fixture ambiguous: {} unconsumed scripts with different needles match \
                     this request's prompt (`match_prompt_contains`: {}) — make the needles \
                     tell them apart",
                    matching.len(),
                    needles.join(", ")
                ),
            });
        }
        Ok(matching.first().map(|(i, _)| *i))
    }

    /// The next unconsumed script that never opted into matching by
    /// prompt at all, in declaration order. This is the whole behavior
    /// for every fixture that doesn't use `match_prompt_contains`.
    fn unnamed(&self, consumed: &[bool]) -> Option<usize> {
        self.fixture
            .sessions
            .iter()
            .enumerate()
            .find(|(i, script)| {
                consumed.get(*i) != Some(&true) && script.match_prompt_contains.is_none()
            })
            .map(|(i, _)| i)
    }

    /// The id the session reports: the one being resumed, or the next of
    /// this adapter's own.
    fn session_id(&self, resume_as: Option<SessionId>) -> Result<SessionId> {
        match resume_as {
            Some(session_id) => Ok(session_id),
            None => SessionId::try_from(format!(
                "mock-session-{}",
                self.next_session.fetch_add(1, Ordering::Relaxed)
            ))
            .map_err(|error| AdapterError::Adapter {
                adapter: ID.clone(),
                message: error.to_string(),
            }),
        }
    }

    /// The effects this request's edit constraints kept from landing —
    /// what the session reports as refused edits.
    fn blocked_markers(&self, script: &SessionScript, req: &SessionRequest) -> Vec<PathBuf> {
        script
            .effects
            .iter()
            .filter(|e| self.is_blocked(req, &e.path))
            .map(|e| e.path.clone())
            .collect()
    }
}

/// Appends to one of the mock's records of what it was asked.
///
/// A poisoned lock is taken back rather than panicked on: a test thread
/// that already failed must not turn every later session into a second
/// failure with nothing to do with the first.
fn record<T>(into: &Mutex<Vec<T>>, value: T) {
    into.lock().unwrap_or_else(|e| e.into_inner()).push(value);
}

/// One of the mock's records, as a test reads it back.
fn read<T: Clone>(from: &Mutex<Vec<T>>) -> Vec<T> {
    from.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub struct MockSession {
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    interrupt: Arc<Notify>,
    kill: Arc<Notify>,
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
        self.interrupt.notify_one();
        Ok(())
    }

    async fn kill(&mut self) -> Result<()> {
        self.kill.notify_one();
        Ok(())
    }
}
