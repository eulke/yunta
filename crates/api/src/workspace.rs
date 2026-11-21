//! See [`crate`]. What an interface hands the API instead of the API
//! reading its process.

use std::path::PathBuf;
use std::sync::Arc;

use std::path::Path;

use yunta_core::port::{Forge, ForgeUnanswered};
use yunta_core::{Clock, ConfigLayer, Env, IdSource, SecretSource};

use yunta_engine::process::Supervision;

use crate::project::resolve;
use crate::{Error, Failure, ForgeSource, Project, Stop};

/// What an interface hands the API instead of the API reading its process:
/// where it works, the environment it was started with, and the boundaries
/// every decision is injected through. An interface builds it once, where its
/// process enters, and hands the same one to every operation: a clone
/// shares its boundaries.
#[derive(Clone)]
pub struct Workspace {
    /// The directory the request is about: the repository a command runs in.
    pub dir: PathBuf,
    /// The environment the process started with, read once.
    pub env: Env,
    /// What every instant an operation records is read from.
    pub clock: Arc<dyn Clock>,
    /// What every id an operation mints is drawn from.
    pub ids: Arc<dyn IdSource>,
    /// What a run or a forge reads a secret's value from, by its name.
    pub secrets: Arc<dyn SecretSource>,
    /// Where the forge a run's config declares is reached from, with `secrets`:
    /// [`DeclaredForge`](crate::DeclaredForge) for every interface.
    pub forges: Arc<dyn ForgeSource>,
    /// The binary a detached run, a resume handed on and the rail hook
    /// start: Yunta itself.
    pub binary: PathBuf,
}

impl Workspace {
    /// The project rooted where this workspace works: its config layers
    /// merged, and the roots its runs, checkouts and log live under.
    pub fn project(&self) -> Result<Project, Error> {
        self.project_at(&self.dir)
    }

    /// The project rooted at `dir`, under this workspace's environment: what
    /// a server that answers for another directory, or a test of a project
    /// handed by name, asks for.
    pub fn project_at(&self, dir: &Path) -> Result<Project, Error> {
        let workspace = Workspace {
            dir: dir.to_path_buf(),
            ..self.clone()
        };
        resolve(workspace).map_err(|failed| Error::Failed(Failure::new(failed)))
    }

    /// The forge `config` declares, reached from this workspace's forges with its
    /// secrets; why none answers where none does.
    pub(crate) fn forge(&self, config: &ConfigLayer) -> Result<Arc<dyn Forge>, ForgeUnanswered> {
        self.forges.forge(config, self.secrets.as_ref())
    }

    /// What a process an operation starts outside any run answers to: the
    /// caller's stop, this workspace's clock, and the environment and path
    /// it was started with.
    pub(crate) fn supervision<'a>(&'a self, stop: &'a Stop) -> Supervision<'a> {
        Supervision::outside_any_run(stop.work(), self.clock.as_ref())
            .with_env(&self.env.subprocess_vars)
            .with_path(&self.env.path)
    }

    /// What gives back what a stopped operation held runs under: the
    /// caller's second stage, since something that runs *because* the first
    /// stage fired cannot answer to it.
    pub(crate) fn teardown<'a>(&'a self, stop: &'a Stop) -> Supervision<'a> {
        Supervision::outside_any_run(stop.teardown(), self.clock.as_ref()).with_path(&self.env.path)
    }
}
