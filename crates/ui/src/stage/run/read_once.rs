//! See [`super`]. What a run's scene reads beyond the run's log as it comes to stand, never on
//! a draw: each reading asked once for what it reads, kept with what it came to once
//! answered, and asked again only for something else. A stage opened over the run's leaves
//! its reading under way, whose answer the run's stage takes under it. The pull request a
//! gate waits on is read so for each pause (`pull_request`), and a finished run's event chain
//! for each run (`receipt`).
//!
//! Each reading is work the performer carries out, so the stage stays a function of what it
//! is handed.

use super::{Framed, RunStage};
use crate::stage::Want;

/// A reading a scene asks beyond the run's log: what it reads, and what it came to once
/// answered. Not the work the performer carries out, which the state waits on and lets go.
#[derive(Debug, Clone)]
pub(super) struct ReadOnce<K, T> {
    /// What it was last asked for, and its answer once it came: none before it is asked.
    read: Option<(K, Option<T>)>,
}

impl<K, T> Default for ReadOnce<K, T> {
    fn default() -> Self {
        ReadOnce { read: None }
    }
}

impl<K: PartialEq, T> ReadOnce<K, T> {
    /// Whether it is asked for `of`: not where it was asked for `of` already, answered or
    /// under way. Asked, it stands under way for `of`, in place of a reading of anything
    /// else.
    pub(super) fn asked(&mut self, of: K) -> bool {
        if self.read.as_ref().is_some_and(|(read, _)| *read == of) {
            return false;
        }
        self.read = Some((of, None));
        true
    }

    /// `answer`, what the reading under way came to, kept for what it was asked for.
    pub(super) fn answered(&mut self, answer: T) {
        if let Some((_, kept)) = self.read.as_mut() {
            *kept = Some(answer);
        }
    }

    /// What it came to, where it was answered for `of`.
    pub(super) fn answer(&self, of: &K) -> Option<&T> {
        match &self.read {
            Some((read, answer)) if read == of => answer.as_ref(),
            Some(_) | None => None,
        }
    }
}

impl RunStage {
    /// What the scene the run `framed` stands as reads beyond the run's log, where it came
    /// to stand for what it was not read for: the pull request it waits on, polled on its
    /// forge for the pause it stands at, and the event chain of a run whose receipt it shows,
    /// verified in its store for the run.
    pub(super) fn read_once(&mut self, framed: Option<&Framed<'_>>) -> Vec<Want> {
        let polled = self.pull_request_asked(framed);
        polled.into_iter().chain(self.chain_asked(framed)).collect()
    }
}
