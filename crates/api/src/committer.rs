//! See [`crate`]. Who a run's commits are made as, asked of git before the
//! run exists: every node's work becomes a commit, so a run in a repository
//! git cannot commit in would fail at its first one, after spending on it.

use crate::{Error, Stop, Workspace};

/// Who git commits as in a repository, or that it can name no one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Committer {
    /// Who git commits as.
    Named(String),
    /// git names no one: what it said, whose last line is its reason.
    Unnamed { git_said: String },
}

impl Workspace {
    /// Who git commits as in this workspace's repository. A stop while git
    /// answers is the caller's stop.
    pub async fn committer(&self, stop: &Stop) -> Result<Committer, Error> {
        match yunta_engine::git::committer(&self.dir, self.supervision(stop)).await {
            Ok(who) => Ok(Committer::Named(who)),
            Err(stopped) if stopped.cancelled() => Err(Error::Stopped),
            Err(unnamed) => Ok(Committer::Unnamed {
                git_said: unnamed.detail(),
            }),
        }
    }
}
