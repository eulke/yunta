//! The `mock` adapter — a first-class adapter, not a test helper: it
//! reproduces sessions from YAML fixtures (a script of events plus
//! filesystem effects), with injectable failures and latency, so the
//! engine's whole cycle — tasks, degradation, cancellation, resume,
//! eventually parallelism — is testable without an LLM. A fixture
//! scripts every session of a run in spawn order; see [`MockFixture`].
//!
//! A fixture names the CLI it stands for. The mock plans each launch with
//! that CLI's own planner, so its argv, mounts, sandbox and coverage are the
//! ones the real adapter builds, and it prints each step as that CLI prints
//! it and reads the lines back with that CLI's reader.
//!
//! Spawning a session is claiming a script and handing it to
//! [`script::play`], which owns everything after that: the adapter
//! decides *which* script a request gets and what the session is born
//! with, the player decides what the session then does. What the adapter
//! keeps is the record of what it was asked to mount, which is how an
//! engine test proves a skill, an agent or a run-tools endpoint reached
//! the session without a real CLI.

mod check;
mod effects;
mod fixture;
mod launch;
mod run_tool;
mod script;
mod session;

pub use fixture::{
    FixtureError, MockEffect, MockFixture, MockOutcome, MockStep, OnInterrupt, RunPaths,
    SessionScript, ToolExpectation,
};

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use yunta_core::port::{
    Adapter, AgentSession, ProbeReport, RailCodec, RunToolsEndpoint, SessionRequest,
};
use yunta_core::process::subprocess::{Held, LineParser};
use yunta_core::{
    AdapterError, AdapterId, AdapterSettings, AgentName, Capabilities, FenceLevel, ModelName,
    Result, ScopeGlob, SessionId,
};

use crate::dialect::DialectName;
use crate::session::typed_settings;

/// The id config names this adapter by.
pub static ID: AdapterId = AdapterId::from_static("mock");

pub struct MockAdapter {
    fixture: MockFixture,
    /// The settings the adapter was built with. The planner of the
    /// fixture's CLI reads them for every session, as the real adapter does.
    settings: AdapterSettings,
    /// What `capabilities()` answers: the fixture's dialect declares them,
    /// and `narrowed` replaces them.
    capabilities: Capabilities,
    /// One flag per `fixture.sessions` entry — `true` once `spawn()` has
    /// claimed it. Replaces a bare atomic counter: concurrent task
    /// dispatch races several `spawn()` calls at once, so "the next
    /// index" stops meaning "the right script" — see
    /// `SessionScript::match_prompt_contains`.
    consumed: Mutex<Vec<bool>>,
    /// Every `spawn()`'s `req.skills`, in claim order: the mock's
    /// "native mount" is recording what it was asked to mount, so
    /// engine tests assert the whole resolution chain without a CLI.
    skills_seen: Mutex<Vec<Vec<PathBuf>>>,
    /// Every `spawn()`'s `req.agent`, in claim order — same
    /// record-the-mount principle as `skills_seen`.
    agents_seen: Mutex<Vec<Option<AgentName>>>,
    /// Every `spawn()`'s `req.model`, in claim order — same
    /// record-the-mount principle as `skills_seen`: the model a session
    /// runs on is the runner's, and a test reads it here rather than
    /// through a real CLI.
    models_seen: Mutex<Vec<Option<ModelName>>>,
    /// Every `resume()`'s session id, in call order: the mock's
    /// "resume" is serving the next script under the SAME session id —
    /// recording which one proves the engine handed back the
    /// conversation it meant to continue.
    resumes_seen: Mutex<Vec<SessionId>>,
    /// Every session's `req.run_tools_endpoint`, in claim order — same
    /// record-the-mount principle as `skills_seen`: engine tests prove
    /// the endpoint reached the session (or deliberately didn't)
    /// without a real CLI.
    endpoints_seen: Mutex<Vec<Option<RunToolsEndpoint>>>,
    /// Every session's `req.artifact_dir`, in claim order — same
    /// record-the-mount principle as `skills_seen`: engine tests prove
    /// which sessions were granted the run's artifact directory without
    /// a real CLI.
    artifact_dirs_seen: Mutex<Vec<Option<PathBuf>>>,
    /// Every request a session was opened with, in claim order — the
    /// whole contract, for a test that asserts two sessions were opened
    /// by the same door rather than field by field.
    requests_seen: Mutex<Vec<SessionRequest>>,
    /// The argv each claimed session's launch plan gives its CLI, in claim
    /// order.
    argvs_seen: Mutex<Vec<Vec<String>>>,
    /// The next session id's number: every adapter counts from one, so
    /// a fixture's ids never depend on what else ran in the process.
    next_session: AtomicU64,
    /// Whether any session of this adapter was asked how its process
    /// ended. A mock has no process, so what this records is the
    /// engine's own rule: the question is put only to a session whose
    /// stream ended saying nothing.
    interrogated: Arc<AtomicBool>,
    /// How many times its service was probed.
    probes: AtomicU64,
}

impl MockAdapter {
    /// One adapter for a fixture, built with default settings.
    pub fn new(fixture: MockFixture) -> Self {
        Self::with_settings(fixture, AdapterSettings::default())
    }

    /// One adapter for a fixture, built with the settings a real adapter of
    /// that CLI would be given.
    pub fn with_settings(fixture: MockFixture, settings: AdapterSettings) -> Self {
        let capabilities = fixture.dialect().capabilities();
        let consumed = Mutex::new(vec![false; fixture.sessions.len()]);
        Self {
            fixture,
            settings,
            capabilities,
            consumed,
            skills_seen: Mutex::new(Vec::new()),
            agents_seen: Mutex::new(Vec::new()),
            models_seen: Mutex::new(Vec::new()),
            resumes_seen: Mutex::new(Vec::new()),
            endpoints_seen: Mutex::new(Vec::new()),
            artifact_dirs_seen: Mutex::new(Vec::new()),
            requests_seen: Mutex::new(Vec::new()),
            argvs_seen: Mutex::new(Vec::new()),
            next_session: AtomicU64::new(1),
            interrogated: Arc::new(AtomicBool::new(false)),
            probes: AtomicU64::new(0),
        }
    }

    /// The same mock, answering with `capabilities` instead of its dialect's,
    /// for a test of how the engine copes with a narrower CLI.
    pub fn narrowed(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Whether any session this adapter opened was asked how it exited.
    pub fn interrogated(&self) -> bool {
        self.interrogated.load(Ordering::SeqCst)
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

    /// The `model` of every session spawned so far, in claim order.
    pub fn models_seen(&self) -> Vec<Option<ModelName>> {
        read(&self.models_seen)
    }

    /// The `skills` of every session spawned so far, in claim order.
    pub fn skills_seen(&self) -> Vec<Vec<PathBuf>> {
        read(&self.skills_seen)
    }

    /// The `run_tools_endpoint` of every session so far, in claim order.
    pub fn endpoints_seen(&self) -> Vec<Option<RunToolsEndpoint>> {
        read(&self.endpoints_seen)
    }

    /// The `artifact_dir` of every session so far, in claim order — what a
    /// test reads to see which sessions were granted the run's artifact
    /// directory and which never needed it.
    pub fn artifact_dirs_seen(&self) -> Vec<Option<PathBuf>> {
        read(&self.artifact_dirs_seen)
    }

    /// The whole request of every session opened so far, in claim order.
    pub fn requests_seen(&self) -> Vec<SessionRequest> {
        read(&self.requests_seen)
    }

    /// The argv of every claimed session's launch plan, in claim order.
    pub fn argvs_seen(&self) -> Vec<Vec<String>> {
        read(&self.argvs_seen)
    }

    /// One adapter from a fixture that names no run directory — every
    /// caller with a run in hand goes through [`MockFixture::parse`]
    /// instead, and a fixture that names a directory is refused here.
    pub fn from_yaml(yaml: &str) -> std::result::Result<Self, FixtureError> {
        Ok(Self::new(MockFixture::parse_without_a_run(yaml)?))
    }
}

#[async_trait]
impl Adapter for MockAdapter {
    fn id(&self) -> &'static AdapterId {
        &ID
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn own_config(&self) -> Vec<ScopeGlob> {
        self.fixture.dialect().own_config()
    }

    /// The settings are read the way the fixture's CLI reads them, so a key
    /// it does not know is an error here, as it is for the real adapter.
    async fn probe(&self) -> Result<ProbeReport> {
        typed_settings::<serde::de::IgnoredAny>(
            &ID,
            self.settings.adapter_settings.as_ref(),
            self.fixture.dialect().known_settings(),
        )?;
        Ok(ProbeReport::Healthy {
            version: Some("mock-0.1".to_string()),
        })
    }

    /// Down for as many probes as the fixture says, then up.
    async fn reachable(
        &self,
        _req: &SessionRequest,
        _shell: &std::collections::BTreeMap<String, String>,
    ) -> Option<bool> {
        let probed = self.probes.fetch_add(1, Ordering::SeqCst);
        Some(probed >= u64::from(self.fixture.service_down_for))
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

    /// The mounts the fixture's CLI would make for this request, as a plan
    /// makes them. A request the CLI would refuse has none.
    fn staged_paths(&self, req: &SessionRequest) -> Vec<PathBuf> {
        self.plan(req, None)
            .map(|plan| plan.mounts.staged())
            .unwrap_or_default()
    }

    /// The codec of the fixture's CLI, for a fixture whose fence is judged per
    /// tool call; there is no codec otherwise.
    fn rail_codec(&self) -> Option<&dyn RailCodec> {
        match self.capabilities.fence {
            FenceLevel::ToolCalls => self.fixture.dialect().rail_codec(),
            FenceLevel::None | FenceLevel::Filesystem => None,
        }
    }
}

impl MockAdapter {
    /// Plans the launch this request asks for, claims the script it gets and
    /// opens the session that plays it.
    fn spawn_scripted(
        &self,
        req: SessionRequest,
        resume_as: Option<SessionId>,
    ) -> Result<Box<dyn AgentSession>> {
        let dialect = self.fixture.dialect();
        // The launch is planned before anything is claimed, so a refused plan
        // consumes no script and writes nothing.
        let plan = self.plan(&req, resume_as.as_ref())?;
        self.record_mount(&req);
        let index = self.claim(&req)?;
        let Some(script) = self.fixture.sessions.get(index) else {
            return Err(AdapterError::Adapter {
                adapter: ID.clone(),
                message: "internal: claimed a session index the fixture does not hold".to_string(),
            });
        };
        plan.write_scratch(&ID)?;
        let reader = dialect.reader(&plan);
        let held: Box<dyn Held> = Box::new(plan.mounts.install(&ID)?);
        record(&self.argvs_seen, plan.args);
        // A CLI that judges each write asks the engine as the session plays;
        // any other session has its effects judged here, before it opens.
        if !self.asks_per_write() {
            self.apply_effects(script, &req)?;
        }
        let played = self.played(script, &req, dialect, reader, resume_as)?;
        Ok(session::start(played, held, Arc::clone(&self.interrogated)))
    }

    /// Whether this mock's CLI judges each write as the session makes it, by
    /// asking the engine's rail.
    fn asks_per_write(&self) -> bool {
        self.capabilities.fence == FenceLevel::ToolCalls
    }

    /// The script a session plays, cut loose from the fixture: what it
    /// prints, what it writes and how, and what it does after.
    fn played(
        &self,
        script: &SessionScript,
        req: &SessionRequest,
        dialect: DialectName,
        reader: Box<dyn LineParser>,
        resume_as: Option<SessionId>,
    ) -> Result<script::Script> {
        let rail = req.rail.clone();
        Ok(script::Script {
            dialect,
            session_id: self.session_id(resume_as)?,
            model: script.model.clone(),
            reader,
            naming: self.capabilities.tool_naming,
            within: RunToolsEndpoint::call_timeout(&req.budget),
            asked: rail
                .clone()
                .filter(|_| self.asks_per_write())
                .map(|rail| effects::Asked {
                    dialect,
                    rail,
                    cwd: req.cwd.clone(),
                    effects: script.effects.clone(),
                    formats: req.formats,
                }),
            before_stop: rail.zip(script.before_stop.as_ref()).map(|(rail, stop)| {
                script::BeforeStop {
                    writes: effects::Asked {
                        dialect,
                        rail,
                        cwd: req.cwd.clone(),
                        effects: stop.writes.clone(),
                        formats: req.formats,
                    },
                    steps: stop.steps.clone(),
                }
            }),
            steps: script.steps.clone(),
            outcome: script.outcome.clone(),
            run_tools_endpoint: req.run_tools_endpoint.clone(),
            lists_tools: self.fixture.lists_tools,
        })
    }

    /// Records what this session was asked to mount, in claim order.
    fn record_mount(&self, req: &SessionRequest) {
        record(&self.skills_seen, req.skills.clone());
        record(&self.endpoints_seen, req.run_tools_endpoint.clone());
        record(&self.artifact_dirs_seen, req.artifact_dir.clone());
        record(
            &self.agents_seen,
            req.agent.as_ref().map(|agent| agent.name.clone()),
        );
        record(&self.models_seen, req.model.clone());
        record(&self.requests_seen, req.clone());
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
                        "fixture exhausted: {}, none left unconsumed and matching this \
                         request — add a session to the fixture for every session the run \
                         opens",
                        yunta_core::text::counted(self.fixture.sessions.len(), "scripted session"),
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
