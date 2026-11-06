//! What a run's sessions said, held for whoever listens.
//!
//! A session's words reach the engine as the notes its adapter streams. The
//! log keeps only a note's size and digest; the words stop here. A run keeps
//! the last few things its sessions said in memory, and hands each new one to
//! every reader the moment it is said. Nothing here is written to disk and
//! nothing here is evidence: a frame changes no state, and replay never sees
//! one.
//!
//! **The voices.** [`Voices`] keeps the recent past under one lock and hands
//! what comes next through a broadcast. A reader copies the past and
//! subscribes under that same lock, so it gets each frame exactly once.
//! Handing a frame over never waits: a reader that falls behind loses frames,
//! and the next one it gets says how many. One channel per reader is what this
//! rejects: it would count a reader's losses by hand, and every handover would
//! walk the readers on the run's own task.
//!
//! **The line.** Between processes a frame is one JSON object per line, its
//! version first. A reader skips a line of a version it does not know and
//! keeps reading, so an engine newer than its reader costs words, never the
//! reading.
//!
//! Not `run_tools::words`, which is what the engine says to an agent: this is
//! what an agent said.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use yunta_core::{NodeId, RunId, Seq, SessionId, TaskId};

/// The version of the line this engine writes. A reader skips a line of any
/// other.
pub const LINE_VERSION: u32 = 1;

/// The most frames the recent past keeps: one for each working node's row
/// with room to spare, and a screenful of history for a reader that
/// connects late.
const KEPT_FRAMES: usize = 64;

/// The most text the recent past keeps, in bytes: sixteen frames of the
/// longest text a frame carries. A busy real run says about 4 KB in two and
/// a half minutes, so this holds minutes of it.
const KEPT_BYTES: usize = 256 * 1024;

/// The longest text one frame carries, in bytes. A session that says more at
/// once is writing a document, and a reader that shows a sentence of it
/// needs none of the rest.
pub const TEXT_MOST: usize = 16 * 1024;

/// How many frames a reader may fall behind before it loses them: four times
/// the recent past, so a reader that has just taken the past has room for a
/// burst. A power of two, which is what the broadcast rounds its room up to.
const READER_QUEUE: usize = 4 * KEPT_FRAMES;

/// What one session said, as a run hands it to whoever listens: redacted
/// before it leaves the engine, never on disk and never evidence. Not the
/// note's digest, which is what the log keeps of the same words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Said {
    pub run_id: RunId,
    pub node_id: NodeId,
    pub session_id: SessionId,
    /// The task the session works, when a loop opened it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// The seq of the note's digest on the log: where this sits among the
    /// run's events.
    pub seq: Seq,
    /// When it was said, by the run's clock.
    pub at: DateTime<Utc>,
    pub text: String,
    /// Whether `text` was cut to [`TEXT_MOST`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
}

impl Said {
    /// This frame with its text cut at the last character boundary at or
    /// under [`TEXT_MOST`], saying so when it was.
    fn bounded(mut self) -> Self {
        if self.text.len() > TEXT_MOST {
            let mut end = TEXT_MOST;
            while !self.text.is_char_boundary(end) {
                end -= 1;
            }
            self.text.truncate(end);
            self.cut = true;
        }
        self
    }
}

/// What a session said as its stream hands it over, already redacted, before
/// the run names itself and the time: the run's engine makes a [`Said`] of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Spoken {
    pub session_id: SessionId,
    pub task_id: Option<TaskId>,
    /// The seq the note's digest took on the log.
    pub seq: Seq,
    pub text: String,
}

impl Spoken {
    /// This, said by `node` of `run` at `at`.
    pub fn said(self, run_id: RunId, node_id: NodeId, at: DateTime<Utc>) -> Said {
        Said {
            run_id,
            node_id,
            session_id: self.session_id,
            task_id: self.task_id,
            seq: self.seq,
            at,
            text: self.text,
            cut: false,
        }
    }
}

/// What one reader is handed: a frame, and how many frames it lost just
/// before it because it fell behind.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub said: Said,
    pub missed: u64,
}

/// One frame as a line carries it, its version first.
#[derive(Serialize, Deserialize)]
struct Line {
    v: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    missed: u64,
    #[serde(flatten)]
    said: Said,
}

/// Only the version of a line, read before anything else in it.
#[derive(Deserialize)]
struct Versioned {
    v: u32,
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

impl Heard {
    /// The line this frame travels as between processes, with no line break
    /// of its own.
    pub fn line(&self) -> String {
        let line = Line {
            v: LINE_VERSION,
            missed: self.missed,
            said: self.said.clone(),
        };
        // Every field is a string, a number or a time, which JSON always
        // holds, so the empty line stands for a failure that cannot occur.
        serde_json::to_string(&line).unwrap_or_default()
    }

    /// The frame `line` carries. `None` for a line of another version, which
    /// the reader skips; an error for a line that is not a frame at all.
    pub fn read(line: &str) -> Result<Option<Heard>, serde_json::Error> {
        if serde_json::from_str::<Versioned>(line)?.v != LINE_VERSION {
            return Ok(None);
        }
        let line: Line = serde_json::from_str(line)?;
        Ok(Some(Heard {
            said: line.said,
            missed: line.missed,
        }))
    }
}

/// What a run's sessions said lately, and the readers listening for more.
pub struct Voices {
    past: Mutex<Past>,
    sender: broadcast::Sender<Arc<Said>>,
}

/// The recent past, and how many bytes of text it holds.
#[derive(Default)]
struct Past {
    frames: VecDeque<Arc<Said>>,
    bytes: usize,
}

impl Voices {
    /// A run's voices, before anything was said.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(READER_QUEUE);
        Voices {
            past: Mutex::new(Past::default()),
            sender,
        }
    }

    /// Keeps `said` in the recent past and hands it to every reader. Never
    /// waits: a reader with no room loses it, and is told so with its next.
    pub fn hand(&self, said: Said) {
        let said = Arc::new(said.bounded());
        let mut past = self.lock();
        past.bytes += said.text.len();
        past.frames.push_back(Arc::clone(&said));
        while past.frames.len() > KEPT_FRAMES || past.bytes > KEPT_BYTES {
            let Some(oldest) = past.frames.pop_front() else {
                break;
            };
            past.bytes -= oldest.text.len();
        }
        // A send with no reader is not a failure: nobody is listening yet.
        let _sent = self.sender.send(said);
    }

    /// A new reader: the recent past, then each frame as it is handed over.
    pub fn listen(&self) -> Listening {
        let past = self.lock();
        Listening {
            past: past.frames.clone(),
            receiver: self.sender.subscribe(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Past> {
        self.past.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for Voices {
    fn default() -> Self {
        Self::new()
    }
}

/// One reader of a run's voices.
pub struct Listening {
    past: VecDeque<Arc<Said>>,
    receiver: broadcast::Receiver<Arc<Said>>,
}

impl Listening {
    /// The next frame: the recent past first, then each one as it is said.
    /// `None` once the voices are gone and nothing is left to hand.
    pub async fn next(&mut self) -> Option<Heard> {
        if let Some(said) = self.past.pop_front() {
            return Some(heard(&said, 0));
        }
        let mut missed = 0;
        loop {
            match self.receiver.recv().await {
                Ok(said) => return Some(heard(&said, missed)),
                Err(broadcast::error::RecvError::Lagged(lost)) => missed += lost,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

fn heard(said: &Said, missed: u64) -> Heard {
    Heard {
        said: said.clone(),
        missed,
    }
}

mod listen;
mod socket;

pub use listen::{connect, Hearing};
pub use socket::{socket_path, Socket};

#[cfg(test)]
mod tests;
