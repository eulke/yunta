//! The shared prologue every subcommand opens with. `Context::load`
//! resolves the project rooted at the current directory once — the single
//! place `project::resolve` is called for a real command — and carries
//! the injected clock and id source with it, so the composition root
//! where cwd, config, state paths, time and identity come together is one
//! place, not seventeen. Storage handles and the adapter registry a
//! command needs are opened from it on demand, since not every command
//! touches the event log or the agents.

use std::path::PathBuf;
use yunta_core::fence::FenceHook;

use yunta_core::events::StoredEvent;
use yunta_core::persisted::PersistedDoc;
use yunta_core::{Manifest, RunId, SystemClock, SystemIdSource};
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
    /// This binary, as the hook a CLI runs to ask the judge about one
    /// write. Resolved once here, so nothing below the shell reads the
    /// process to find out where it lives.
    pub fence_hook: FenceHook,
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
            fence_hook: fence_hook(),
        })
    }

    /// The same project with its state roots moved under `sandbox` and
    /// its work happening in `worktree` — what `yunta test` runs a case
    /// in.
    ///
    /// The config is this project's real one, layers and all, because a
    /// case exists to exercise the workflow the repo actually declares;
    /// everything a run *writes* goes under the sandbox instead, so a
    /// case leaves nothing behind and two cases never meet. The clock
    /// and the id source are the invocation's own: a case is scripted
    /// in what its sessions do, not in when they happened.
    pub fn sandboxed(&self, worktree: PathBuf, sandbox: &std::path::Path) -> Self {
        Self {
            cwd: worktree,
            project: Project {
                config: self.project.config.clone(),
                runs_root: sandbox.join("runs"),
                worktrees_root: sandbox.join("worktrees"),
                storage_path: sandbox.join("events.db"),
            },
            clock: self.clock,
            ids: self.ids,
            fence_hook: self.fence_hook.clone(),
        }
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

    /// Everything a command needs to say something about one run: where
    /// it lives, the manifest it froze, and its whole log.
    ///
    /// The one prologue every run-opening command shares. Twelve of them
    /// each found the directory, read the manifest and loaded the events
    /// their own way, and each wrote its own sentence for a run that is
    /// not there — so a person who mistyped an id was told a different
    /// thing by `status` than by `receipt`. One prologue means one
    /// sentence, and a run that is open is open the same way everywhere.
    pub async fn open_run(&self, id: &RunId) -> Result<Opened, CliError> {
        let events = self
            .async_storage()
            .await?
            .events_for_run(id.clone())
            .await?;
        let run_dir = self.project.run_dir(id.as_str());
        if events.is_empty() && run_dir.is_none() {
            return Err(CliError::RunNotFound {
                id: id.clone(),
                roots: self.run_roots(),
            });
        }
        let run_dir = run_dir.unwrap_or_else(|| self.project.runs_root.join(id.as_str()));
        let manifest = crate::load_manifest(&yunta_engine::run_dir::manifest_path(&run_dir))?;
        Ok(Opened {
            run_id: id.clone(),
            run_dir,
            manifest,
            events,
        })
    }

    /// Where a run is looked for, in search order — what a refusal names
    /// so a person knows where this binary did look.
    pub fn run_roots(&self) -> Vec<PathBuf> {
        let here = self.project.runs_root.clone();
        let default = project::user_root().ok().map(|root| root.join("runs"));
        std::iter::once(here.clone())
            .chain(default.filter(|default| *default != here))
            .collect()
    }
}

/// One run, open: what every command that says something about a run
/// needs before it can.
pub struct Opened {
    /// The run this is, so a caller that passes `Opened` on does not
    /// carry the id beside it.
    #[allow(dead_code)]
    pub run_id: RunId,
    pub run_dir: PathBuf,
    pub manifest: PersistedDoc<Manifest>,
    pub events: Vec<StoredEvent>,
}

/// Where this binary lives, for the child processes that run it back:
/// the fence hook, and the detached `resume` a `--detach` run spawns. A
/// host that will not say falls back to the name on `PATH`.
pub fn own_binary() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("yunta"))
}

/// The hook a CLI runs to ask the judge about one write. Resolved in
/// the shell — every path below it takes the value, never the process.
pub fn fence_hook() -> FenceHook {
    FenceHook::new(own_binary())
}
