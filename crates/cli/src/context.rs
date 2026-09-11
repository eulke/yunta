//! The shared prologue every subcommand opens with. `Context::load`
//! resolves the project rooted at the current directory once — the single
//! place `project::resolve` is called for a real command — and carries
//! the injected clock and id source with it, so the composition root
//! where cwd, config, state paths, time and identity come together is one
//! place, not seventeen. Storage handles and the adapter registry a
//! command needs are opened from it on demand, since not every command
//! touches the event log or the agents.

use std::path::PathBuf;

use yunta_core::{SystemClock, SystemIdSource};
use yunta_storage::{AsyncStorage, Storage};

use crate::error::CliError;
use crate::project::{self, Project};

/// Everything a subcommand resolves before it can act: the current
/// directory, the project rooted there, and the injected clock and id
/// source a run mints its ids and stamps its events from.
pub struct Context {
    pub cwd: PathBuf,
    pub project: Project,
    pub clock: SystemClock,
    pub ids: SystemIdSource,
}

impl Context {
    /// Reads the current directory and resolves the project there — the
    /// prologue a command run from a shell shares.
    pub fn load() -> Result<Self, CliError> {
        let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
        Self::resolve_in(cwd)
    }

    /// Resolves the project rooted at an explicit directory — what the
    /// control plane (`yunta mcp`) and `yunta test --dir` need, since
    /// they act on a directory they were handed, not the one they run in.
    /// The single place `project::resolve` is called.
    pub fn resolve_in(cwd: PathBuf) -> Result<Self, CliError> {
        let project = project::resolve(&cwd)?;
        Ok(Self {
            cwd,
            project,
            clock: SystemClock,
            ids: SystemIdSource,
        })
    }

    /// The async event-log handle `run`, `resume`, `cancel` and `status`
    /// drive the engine and read derived state through.
    pub async fn async_storage(&self) -> Result<AsyncStorage, CliError> {
        Ok(AsyncStorage::open(&self.project.storage_path).await?)
    }

    /// The blocking event-log handle the synchronous commands (`gc`,
    /// `verify`, `receipt`, `stats`) read through.
    pub fn storage(&self) -> Result<Storage, CliError> {
        Ok(Storage::open(&self.project.storage_path)?)
    }

    /// The adapter registry this project's `runners:` names, each built
    /// with its own configured settings — the real agents a run can use.
    pub fn adapters(&self) -> crate::commands::Adapters {
        crate::commands::real_adapters(&self.project.config)
    }
}
