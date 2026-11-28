//! See [`super`]. The successor's side of a look: the run a promotion handed the work of
//! the run held to, found by the first event of each run of the log through the
//! following's one reader, read whole and followed in its place.
//!
//! A run held that closed promoted awaits its successor from the look that reads its
//! close, or from its reading whole, onward. The log is asked then, and again after each
//! commit, every run of it by its first event, so a successor already in the log, or one
//! made between two looks, is handed as one that comes, right after the look that read the
//! close; a successor found closed promoted in turn is asked after at once, along the chain
//! to its end. The chain keeps the runs it held, and a run of them the log names as a
//! successor again, as a run whose birth names itself or a cycle would, is none: the chain
//! stops there, so a look ends and holds each run once. The wait runs from the close, by
//! the instant its event carries, whoever reads it and whenever: a run that closed longer
//! ago than a successor takes to be created, with none in the log, is handed at once as
//! having none, and one that closed a moment ago is awaited for what is left. That none
//! came is handed once and is final: the run is asked after no more, at no later commit.

use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::time::Instant;
use yunta_core::RunId;
use yunta_replay::frame::{RunFrame, RunPhase};

use super::following::Held;
use super::{FollowNews, FollowedRun, Following, RunNews};
use crate::event_log::successor_of;
use crate::run::read_manifest;
use crate::{Error, Failure};

/// How long after a run closed promoted its successor is awaited before a following
/// hands that none came: the engine that promotes creates it once it has closed the run
/// and set its tree up, which takes a few seconds on a large repository.
const SUCCESSOR_WAIT: Duration = Duration::from_secs(30);

/// Where a following stands on the successor of the run it holds.
#[derive(Debug, Clone, Copy)]
pub(super) enum SuccessorWait {
    /// The run held has not closed promoted, as far as it has been read.
    NotPromoted,
    /// The run held closed promoted and no successor of it has been found: the log is
    /// asked for one after each commit, until `until` on tokio's time, which is
    /// [`SUCCESSOR_WAIT`] past the close.
    Awaited { until: Instant },
    /// None came within [`SUCCESSOR_WAIT`] of the close, which was handed once and is
    /// final: the run is asked after no more, and a run that names it later joins the
    /// field as any run new to the log does.
    RanOut,
}

impl SuccessorWait {
    /// The wait begun, as the run's close is first read, with `left` of it to run:
    /// whether it began now.
    fn begin(&mut self, left: Duration) -> bool {
        let began = matches!(self, SuccessorWait::NotPromoted);
        if began {
            *self = SuccessorWait::Awaited {
                until: Instant::now() + left,
            };
        }
        began
    }
}

/// What is left at `now` of the wait for the successor of a run that closed at
/// `closed_at`: the wait runs from the close, whoever reads it and whenever. A close the
/// workspace clock reads as yet to come, or one the log does not stamp, leaves it whole.
fn wait_left(closed_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Duration {
    let since = closed_at.and_then(|closed_at| (now - closed_at).to_std().ok());
    SUCCESSOR_WAIT.saturating_sub(since.unwrap_or(Duration::ZERO))
}

impl Following {
    /// Follows the run held onto its successor while it stands closed promoted, and
    /// hands once that none came when the wait for one runs out, `now` being the
    /// workspace clock's instant. The log is asked as the close is first read, then after
    /// each commit until the wait runs out; a successor found closed promoted in turn is
    /// asked after at once, along the chain.
    pub(super) async fn onto_successors(&mut self, now: DateTime<Utc>) -> Result<(), Error> {
        loop {
            let Some(held) = self.held.as_mut() else {
                return Ok(());
            };
            if !closed_promoted(held.run.frame()) {
                return Ok(());
            }
            let closed_at = held.run.state().run.finished().map(|finished| finished.at);
            let began = held.successor.begin(wait_left(closed_at, now));
            // That none came is final: a run handed as having none is asked after no more.
            let awaited = matches!(held.successor, SuccessorWait::Awaited { .. });
            if !awaited || !(began || self.commit_unread) {
                break;
            }
            let successor = successor_of(&self.reader, held.run.run(), &held.chain).await?;
            let Some(successor) = successor else {
                break;
            };
            self.go_on_with(successor).await?;
        }
        self.ran_out();
        Ok(())
    }

    /// Follows `successor` from here on, read whole, as the field reads it where it does,
    /// else beside the run it was promoted from, since the engine that promotes creates
    /// it under the runs root it works in.
    async fn go_on_with(&mut self, successor: RunId) -> Result<(), Error> {
        let Some(held) = &self.held else {
            return Ok(());
        };
        let predecessor = held.run.run().clone();
        let mut chain = held.chain.clone();
        chain.insert(successor.clone());
        let (read, registry, events) = match self.whole_from_the_field(&successor).await? {
            Some(whole) => whole,
            None => {
                let registry = held.registry.of_successor(&successor);
                let manifest_path = yunta_engine::run_dir::manifest_path(registry.run_dir());
                let manifest = read_manifest(&manifest_path)?.doc;
                let events = self.reader.events(&successor).await.map_err(Failure::new)?;
                let read =
                    FollowedRun::read(successor.clone(), manifest, &events, registry.engine());
                (read, registry, events)
            }
        };
        self.hold(Held {
            run: read.clone(),
            registry,
            successor: SuccessorWait::NotPromoted,
            chain,
        });
        if self.followed.run.as_ref() == Some(&predecessor) {
            self.followed.run = Some(successor);
        }
        let news = RunNews::Successor {
            run: Box::new(read),
            events,
        };
        self.news.push_back(FollowNews::Run {
            run: predecessor,
            news,
        });
        Ok(())
    }

    /// That no successor came, kept to hand back once, when the run held has awaited one
    /// until [`SUCCESSOR_WAIT`] past its close.
    fn ran_out(&mut self) {
        let Some(held) = self.held.as_mut() else {
            return;
        };
        let SuccessorWait::Awaited { until } = held.successor else {
            return;
        };
        if Instant::now() >= until {
            held.successor = SuccessorWait::RanOut;
            self.found(RunNews::NoSuccessor);
        }
    }
}

/// Whether the run `frame` reads closed promoted, handing the work to a successor.
fn closed_promoted(frame: &RunFrame) -> bool {
    matches!(frame.phase, RunPhase::Promoted { .. })
}
