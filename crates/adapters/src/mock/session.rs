//! The handle a played session is returned as: the stream the engine reads,
//! the two stops that end its player, and the hold on what its launch set up.
//! Dropping it ends the player; releasing it takes back the launch's mounts.
//! The adapter's record of the sessions it opened sits here too: what each
//! was asked, which the tests read back, and the ledger of what it claimed
//! when its caller keeps one, so a mock built later goes on from there.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Notify};
use yunta_core::durable::{self, Durability};
use yunta_core::events::SessionExit;
use yunta_core::port::{AgentEvent, AgentSession, RunToolsEndpoint, SessionRequest};
use yunta_core::process::subprocess::Held;
use yunta_core::{AdapterError, AgentName, ModelName, Pid, SessionId};

use super::error::FixtureError;
use super::script::{play, Script, Stops};
use super::stand_in::StandIn;
use super::{read, KillBehavior, MockAdapter, ID, MOCK_SESSION_PREFIX};

/// The session a claimed script is played by: the stream the engine reads
/// and the handles that stop it. Dropping it ends its player, and what its
/// launch set up is taken back.
pub(super) struct MockSession {
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    interrupt: Arc<Notify>,
    kill: Arc<Notify>,
    /// Whether the script refuses `kill` while it still plays: the player
    /// then keeps running.
    refuses_kill: bool,
    /// The process a refused kill leaves running. It is ended once the script
    /// has ended, so a session that finished leaves no process behind.
    stand_in: Option<StandIn>,
    /// Whether the player still plays its script. The player clears it before
    /// its stream closes, so a reader that has seen the stream end sees it too.
    playing: Arc<AtomicBool>,
    /// The task playing this session's script. A session that ends
    /// before its script does — a cancelled run, a caller that drops
    /// the stream — leaves a player waiting on a step that will never
    /// be read, so the session owns it and takes it down with itself.
    player: tokio::task::JoinHandle<()>,
    /// Shared with the adapter that opened this session.
    interrogated: Arc<AtomicBool>,
    /// What the launch set up for this session, taken back on release.
    held: Option<Box<dyn Held>>,
}

/// Starts playing `script` as a session, which holds what its launch set up
/// until it is released.
pub(super) fn start(
    script: Script,
    held: Box<dyn Held>,
    interrogated: Arc<AtomicBool>,
) -> Box<dyn AgentSession> {
    let (events, receiver) = mpsc::unbounded_channel();
    let interrupt = Arc::new(Notify::new());
    let kill = Arc::new(Notify::new());
    let playing = Arc::new(AtomicBool::new(true));
    let stops = Stops::of(&script.outcome, Arc::clone(&interrupt), Arc::clone(&kill));
    let refuses_kill = script.kill == Some(KillBehavior::Refuse);
    let stand_in = refuses_kill.then(StandIn::start).flatten();
    let player = tokio::spawn(play(script, events, stops, Arc::clone(&playing)));
    Box::new(MockSession {
        receiver: Some(receiver),
        interrupt,
        kill,
        refuses_kill,
        stand_in,
        playing,
        player,
        interrogated,
        held: Some(held),
    })
}

impl MockSession {
    /// Ends the stand-in once the script has ended. A session still playing
    /// keeps it: that process is the one the engine could not stop.
    fn end_stand_in_when_done(&mut self) {
        if self.playing.load(Ordering::SeqCst) {
            return;
        }
        if let Some(stand_in) = self.stand_in.take() {
            stand_in.end();
        }
    }
}

impl Drop for MockSession {
    fn drop(&mut self) {
        self.player.abort();
        self.end_stand_in_when_done();
    }
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

    async fn interrupt(&mut self) -> yunta_core::Result<()> {
        self.interrupt.notify_one();
        Ok(())
    }

    /// A kill the script refuses leaves its player running, and says so, for as
    /// long as the player still plays. Once the script has ended there is
    /// nothing left to refuse, so the kill answers as it does for any session.
    async fn kill(&mut self) -> yunta_core::Result<()> {
        if self.refuses_kill && self.playing.load(Ordering::SeqCst) {
            return Err(AdapterError::Adapter {
                adapter: ID.clone(),
                message: "kill was refused: the session keeps running".to_string(),
            });
        }
        self.kill.notify_one();
        self.end_stand_in_when_done();
        Ok(())
    }

    /// The group of the process a refused kill left running, which the
    /// engine's process registry names for as long as it runs.
    fn pgid(&self) -> Option<Pid> {
        self.stand_in.as_ref().map(StandIn::group)
    }

    /// Takes back what the launch set up for this session.
    async fn release(&mut self) -> yunta_core::Result<()> {
        self.end_stand_in_when_done();
        match self.held.take() {
            Some(held) => held.release(),
            None => Ok(()),
        }
    }

    /// Nothing, because there is no process — and a record that the
    /// question was put, which is what a test of the engine's rule
    /// reads.
    async fn exit(&mut self) -> yunta_core::Result<Option<SessionExit>> {
        self.interrogated.store(true, Ordering::SeqCst);
        Ok(None)
    }
}

impl MockAdapter {
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
}

impl MockAdapter {
    /// The same mock, keeping its claims in the file at `path`. A file that
    /// exists is read first: the scripts it claimed are not served again, and
    /// the session numbers go on from where it left off. A write the ledger
    /// needs and cannot make fails the spawn that needed it.
    pub fn keeping_claims(mut self, path: PathBuf) -> Result<Self, FixtureError> {
        self.claims = Mutex::new(Claims::kept(path, self.fixture.sessions.len())?);
        Ok(self)
    }
}

/// What a mock has claimed and how far it has numbered its sessions, as one
/// state, so that every write of the ledger carries both.
pub(super) struct Claims {
    /// One flag per fixture script — `true` once a `spawn()` has claimed it.
    consumed: Vec<bool>,
    /// The number the next fresh session id takes. Every mock counts from one,
    /// so a fixture's ids never depend on what else ran in the process.
    next_session: u64,
    /// The file the ledger is written to, when the caller keeps one. Without
    /// it nothing is written.
    file: Option<PathBuf>,
}

/// The ledger file: the scripts claimed, and the number the next session takes.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    claimed: Vec<usize>,
    next_session: u64,
}

impl Claims {
    /// Nothing claimed and nothing kept: the state of a mock built without a
    /// ledger.
    pub(super) fn fresh(scripts: usize) -> Self {
        Self {
            consumed: vec![false; scripts],
            next_session: 1,
            file: None,
        }
    }

    /// The state a ledger file records, or a fresh one when the file is
    /// missing. A file that cannot be read, is not a ledger, or claims a
    /// script the fixture does not hold is refused.
    pub(super) fn kept(path: PathBuf, scripts: usize) -> Result<Self, FixtureError> {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    file: Some(path),
                    ..Self::fresh(scripts)
                });
            }
            Err(source) => return Err(refused(&path, format!("it cannot be read: {source}"))),
        };
        let ledger: Ledger = serde_json::from_slice(&bytes)
            .map_err(|source| refused(&path, format!("it is not a claims ledger: {source}")))?;
        let mut consumed = vec![false; scripts];
        for index in ledger.claimed {
            let Some(claimed) = consumed.get_mut(index) else {
                return Err(refused(
                    &path,
                    format!("it claims script {index}, which the fixture does not hold"),
                ));
            };
            *claimed = true;
        }
        if ledger.next_session == 0 {
            return Err(refused(
                &path,
                "it numbers its next session from zero".to_string(),
            ));
        }
        Ok(Self {
            consumed,
            next_session: ledger.next_session,
            file: Some(path),
        })
    }

    /// The flags of the scripts claimed so far, one per fixture script.
    pub(super) fn consumed(&self) -> &[bool] {
        &self.consumed
    }

    /// Claims script `index` for the session that plays it, and names that
    /// session: the one being resumed, or a fresh id taken from the next
    /// number. The claim and the number are one change, written to the ledger
    /// before either is kept, so a change the ledger cannot record is not made.
    pub(super) fn claim(
        &mut self,
        index: usize,
        resume_as: Option<SessionId>,
    ) -> Result<SessionId, AdapterError> {
        let mut consumed = self.consumed.clone();
        if let Some(claimed) = consumed.get_mut(index) {
            *claimed = true;
        }
        let (session_id, next_session) = match resume_as {
            Some(session_id) => (session_id, self.next_session),
            None => (fresh_id(self.next_session)?, self.next_session + 1),
        };
        self.persist(&consumed, next_session)?;
        self.consumed = consumed;
        self.next_session = next_session;
        Ok(session_id)
    }

    /// Writes the whole ledger for `consumed` and `next_session`, when a file
    /// is kept. Nothing is written otherwise.
    fn persist(&self, consumed: &[bool], next_session: u64) -> Result<(), AdapterError> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        let ledger = Ledger {
            claimed: claimed_indices(consumed),
            next_session,
        };
        // blocking: the ledger is one small file, and its write is what makes
        // the change it records; it runs under the claims lock, which keeps
        // two writes in the order their changes were made.
        let written = serde_json::to_vec_pretty(&ledger)
            .map_err(|source| io::Error::new(io::ErrorKind::InvalidData, source))
            .and_then(|bytes| durable::replace(file, &bytes, Durability::Atomic));
        written.map_err(|source| AdapterError::AdapterIo {
            adapter: ID.clone(),
            action: format!("write its claims file {}", file.display()),
            source,
        })
    }
}

/// The indices of the scripts `consumed` flags as claimed, in fixture order.
fn claimed_indices(consumed: &[bool]) -> Vec<usize> {
    consumed
        .iter()
        .enumerate()
        .filter_map(|(index, claimed)| claimed.then_some(index))
        .collect()
}

/// The id of a fresh session, numbered `number`.
fn fresh_id(number: u64) -> Result<SessionId, AdapterError> {
    SessionId::try_from(format!("{MOCK_SESSION_PREFIX}{number}")).map_err(|error| {
        AdapterError::Adapter {
            adapter: ID.clone(),
            message: error.to_string(),
        }
    })
}

/// A claims file refused for `reason`, named by its path.
fn refused(path: &Path, reason: String) -> FixtureError {
    FixtureError::Claims {
        path: path.to_path_buf(),
        reason,
    }
}
