//! See [`crate`]. Following what an interface names, while it moves: what is
//! [`Followed`], the field as far as a reach goes and at most one run read whole; the
//! [`Following`] that hands back what comes as [`FollowNews`], at one pace for every
//! interface; the [`FollowedField`] the field is first handed as; the [`FollowedRun`]
//! each piece of [`RunNews`] folds into, which an interface keeps a copy of; the registry
//! its engine keeps beside it, which names the process driving it and the socket its
//! sessions' words are heard on; and those words. Owned, so a task drawing the run holds
//! its own.
//!
//! A look's paces, and what each costs. Every tenth of a second a look asks the log,
//! through one connection the following opens and keeps and which refuses to write,
//! whether another connection committed since the last look: one pragma, the whole cost
//! of a look at a store where nothing moves, whatever the field holds. Only after a
//! commit does it read further: the runs' heads once, when it follows a field, then the
//! events past the head of each run that moved, a statement a run; a run new to the log,
//! its log whole. Once a second, for each run whose log says it moves or asks a person,
//! it reads the registry beside the run, since an engine that dies and a question asked
//! at a terminal commit nothing; every ten seconds, that each run of the field still has
//! its directory. Neither reads the log. Reading a field whole costs the runs' heads and
//! each run's log, and git once for each reach, to say which repository is here. A run
//! closed promoted is asked of the same reader for its successor: each run's first event,
//! in one statement, as its close is read and again only after a commit, until a successor
//! or that none came is handed. Every pace runs on tokio's monotonic time; the workspace
//! clock stamps the frames, and says how long ago a run closed promoted, which the wait
//! for its successor runs from.
//!
//! One fold. The run followed is handed read whole once, with every event of its log
//! from its first ([`FollowNews::Whole`]), then each batch as it comes, and a successor
//! read whole the same way. Whoever follows folds them with replay's own fold: the
//! moments of `chronicle` over the events read whole, then those [`FollowedRun::took`]
//! hands back for each batch, so every interface tells the run as `yunta status` reads
//! it, and nobody folds its whole log again on each piece of news.
//!
//! What the run read whole waits on a person for is read once for each pause, on a task
//! of its own, off the look's path, and handed with the [`AnswerablePause`] it waits at:
//! `awaited`'s. Each [`Document`] its nodes hand over is read once, as the look that
//! reads the event handing it over ends, and the keys of its manifest this binary does
//! not read are said once for each run read whole: `document`'s. Both are read as of the
//! head the following holds the run at, never past it, so no news of a run runs ahead of
//! the events handed of it: what the log took past that head comes in the next batch first.

mod awaited;
mod document;
mod field;
mod followed;
mod following;
mod listening;
mod successor;

use std::path::{Path, PathBuf};

use yunta_core::events::StoredEvent;
use yunta_core::port::Said;
use yunta_core::{NodeId, RunId};
use yunta_engine::said::{Heard, Hearing};
use yunta_render::page::UnreadableRun;
use yunta_replay::registry::Engine;

use crate::{Awaited, FieldReach, OpenedRun, Remark};

pub use awaited::AnswerablePause;
pub use document::Document;
pub use field::FollowedField;
pub use followed::FollowedRun;
pub use following::Following;

/// What a following follows: the field as far as a reach goes, and at most one run read
/// whole, which an interface names and names again as what it shows changes. Not
/// [`FollowedRun`], the run as the following read it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Followed {
    /// The runs as far as it reaches, each at its head; none follows no field.
    pub field: Option<FieldReach>,
    /// The run read whole, with every event of its log, then each batch that comes;
    /// none follows no run.
    pub run: Option<RunId>,
}

/// What a following hands back, run by run. Not [`RunNews`], what came to one run held.
#[derive(Debug, Clone)]
pub enum FollowNews {
    /// The field followed, read whole: as it is first read, and as it is read again at
    /// another reach. It holds every run of the field, so whoever follows knows it holds
    /// the whole field before it decides where it stands.
    Field(Box<FollowedField>),
    /// A run that came into the field after it was read whole, at its head; or a run of
    /// it that could not be read before and is read now.
    Joined(Box<FollowedRun>),
    /// A run of the field that cannot be read, with why, in place of what was held of it:
    /// its log or its manifest failed a read, and it is read again whole when its head
    /// moves; or its directory went, as collection takes a run before its rows.
    Unreadable(UnreadableRun),
    /// A run of the field gone from the log: it leaves the field.
    Left(RunId),
    /// How many runs the field leaves to other repositories, once that changes.
    Elsewhere(usize),
    /// The run followed, read whole: at its head, with every event of its log from its
    /// first, handed once, as it is first read. A run of the field read whole stands at
    /// the head the field holds it at.
    Whole {
        run: Box<FollowedRun>,
        events: Vec<StoredEvent>,
    },
    /// What came to the run `run`, past what was handed of it before: once for the run
    /// read whole and its place in the field alike, so whoever keeps both folds it into
    /// both, and the two never disagree.
    Run { run: RunId, news: RunNews },
}

/// What a look at a followed run found. Not what a run's observer is handed in the
/// process that drives it, which sees each step as it is taken.
#[derive(Debug, Clone)]
pub enum RunNews {
    /// What came past the last event read, in the log's order.
    Events(Vec<StoredEvent>),
    /// The process driving the run, as its registry reads now that it changed.
    Engine(Engine),
    /// What one of the run's sessions said, and how many words its engine dropped for
    /// this following just before it, because the following fell behind: none while it
    /// keeps up.
    Said { said: Said, missed: u64 },
    /// The run a promotion handed the work to, read whole: at its head, with every event
    /// of its log from its first. It is followed from here on, in its predecessor's
    /// place, as the run read whole; the field keeps the predecessor in its own place,
    /// and the successor joins it as any run new to the log does.
    Successor {
        run: Box<FollowedRun>,
        events: Vec<StoredEvent>,
    },
    /// The run closed promoted and no successor came in the time one takes to be
    /// created since its close. Final: no successor of the run is handed after it. Not
    /// `Successor`.
    NoSuccessor,
    /// What the run waits on a person for, read once for each pause it waits at and
    /// handed with that pause; none once it waits on none, as a decision answered
    /// elsewhere reads.
    Awaited(Option<(AnswerablePause, Awaited)>),
    /// What the run waits on a person for at `pause` could not be read, with why: a
    /// document its decision shows fails today's shape, or its round's document cannot be
    /// read. Not `Awaited(None)`: the run still waits.
    AwaitedUnread { pause: AnswerablePause, why: String },
    /// A document the run's node `node` handed over, as the following read it: once, when
    /// the event that hands it over is read, and each one handed over before the run was
    /// read whole, with it.
    Document { node: NodeId, document: Document },
    /// What reading the run remarks on, a manifest's keys this binary does not read: said
    /// once for each run read whole, never at a look.
    Remark(Remark),
}

/// The registry a run's engine keeps in the run's directory: the process
/// driving the run, whom it asks at its terminal, and the socket its
/// sessions' words are heard on.
#[derive(Debug, Clone)]
pub struct RunRegistry {
    dir: PathBuf,
}

impl RunRegistry {
    /// The registry of the run whose directory is `dir`.
    pub fn at(dir: PathBuf) -> Self {
        RunRegistry { dir }
    }

    /// The directory of the run it is the registry of, which keeps the run's manifest
    /// and documents beside it.
    pub fn run_dir(&self) -> &Path {
        &self.dir
    }

    /// The process driving the run as the registry names it and this machine
    /// answers for it now: alive, gone, or never recorded.
    pub fn engine(&self) -> Engine {
        yunta_engine::read_engine(&self.dir, &yunta_engine::lock::SystemProbe)
    }

    /// The registry of `successor`, the run a promotion handed the work to:
    /// beside this one's, since the engine that promotes creates it under the
    /// runs root it works in.
    pub fn of_successor(&self, successor: &RunId) -> Self {
        RunRegistry {
            dir: self.dir.with_file_name(successor.as_str()),
        }
    }
}

impl OpenedRun<'_> {
    /// The registry the run's engine keeps beside it.
    pub fn registry(&self) -> RunRegistry {
        RunRegistry::at(self.dir.clone())
    }
}

/// What a run's sessions say, heard off the socket its engine serves.
struct Words {
    hearing: Hearing,
}

impl Words {
    /// Listens on `socket`, the one a run's registry names: `None` when
    /// nothing serves it, as a socket a gone engine left behind.
    async fn hear(socket: &Path) -> Option<Words> {
        let hearing = yunta_engine::said::connect(socket).await.ok()?;
        Some(Words { hearing })
    }

    /// The next thing a session says, with how many words the engine dropped just before
    /// it for a reader that fell behind; `None` once the engine stops serving.
    async fn next(&mut self) -> Option<Heard> {
        self.hearing.next().await
    }
}

#[cfg(test)]
mod tests;
