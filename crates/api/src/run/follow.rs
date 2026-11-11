//! See [`super`]. Following a run while it moves: the [`Following`] that hands back
//! what comes as [`RunNews`], at one pace for every interface; the [`FollowedRun`] that
//! news folds into, which an interface keeps a copy of; the registry its engine keeps
//! beside it, which names the process driving it and the socket its sessions' words are
//! heard on; and those words. Owned, so a task drawing the run holds its own.

mod followed;
mod following;
mod listening;

use std::path::{Path, PathBuf};

use yunta_core::port::Said;
use yunta_core::RunId;
use yunta_engine::said::Hearing;
use yunta_replay::registry::Engine;

use super::OpenedRun;

pub use followed::FollowedRun;
pub use following::{Following, RunNews};

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

    /// The next thing a session says; `None` once the engine stops serving.
    async fn next(&mut self) -> Option<Said> {
        self.hearing.next().await.map(|heard| heard.said)
    }
}
