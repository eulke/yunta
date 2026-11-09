//! The shared prologue every subcommand opens with. `Context::load`
//! resolves the project rooted at the current directory once — the single
//! place `project::resolve` is called for a real command — and carries
//! the injected clock and id source with it, so the composition root
//! where cwd, config, state paths, time and identity come together is one
//! place, not seventeen. Storage handles and the adapter registry a
//! command needs are opened from it on demand, since not every command
//! touches the event log or the agents.

use std::path::PathBuf;

use std::sync::Arc;
use yunta_core::rail::RailCommand;

use yunta_core::{Clock, IdSource};
use yunta_storage::{AsyncStorage, Storage};

use crate::error::CliError;
use crate::interrupt::Interrupt;
use crate::shell::Shell;
use yunta_api::Project;

/// Everything a subcommand resolves before it can act: the current
/// directory, the project rooted there, and the injected clock and id
/// source a run mints its ids and stamps its events from.
pub struct Context {
    pub cwd: PathBuf,
    pub project: Project,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdSource>,
    /// The ambient environment this invocation runs in, read once here
    /// rather than from the process below: the user state root and the
    /// variables layered onto every subprocess.
    pub env: yunta_core::Env,
    /// What stops this invocation, in two stages. Private: every
    /// command reaches it through the getters below, so nothing below
    /// the shell decides which stage it answers to.
    interrupt: Interrupt,
    /// This binary, as the hook a CLI runs to ask the judge about one
    /// write. Resolved once in the shell, so nothing below it reads the
    /// process to find out where it lives.
    pub rail_command: RailCommand,
    /// This binary, which a detached run and a resume handed on start.
    pub binary: PathBuf,
}

impl Context {
    /// The project rooted where the shell runs: the prologue a command run
    /// from a shell shares.
    pub fn of(shell: &Shell) -> Result<Self, CliError> {
        Self::in_dir(shell, shell.workspace().dir.clone())
    }

    /// The project rooted at `dir`, with the shell's clock, environment and
    /// interrupt: what the control plane and `yunta test --dir` need, since
    /// they act on a directory they were handed. The single place
    /// `project::resolve` is called.
    pub fn in_dir(shell: &Shell, dir: PathBuf) -> Result<Self, CliError> {
        let workspace = shell.workspace();
        let project = workspace.project_at(&dir)?;
        Ok(Self {
            cwd: dir.clone(),
            project,
            clock: Arc::clone(&workspace.clock),
            ids: Arc::clone(&workspace.ids),
            env: workspace.env.clone(),
            interrupt: shell.interrupt(),
            rail_command: shell.rail_command(),
            binary: workspace.binary.clone(),
        })
    }

    /// The token the work of this invocation answers to: the first
    /// interrupt.
    pub fn cancellation(&self) -> &tokio_util::sync::CancellationToken {
        self.interrupt.stop()
    }

    /// What the work runs under: no registry — a command is not a run —
    /// the first stage, and this invocation's clock and environment.
    pub fn supervision(&self) -> yunta_engine::process::Supervision<'_> {
        yunta_engine::process::Supervision::outside_any_run(
            self.cancellation(),
            self.clock.as_ref(),
        )
        .with_env(&self.env.subprocess_vars)
        .with_path(&self.env.path)
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
            cwd: worktree.clone(),
            project: Project {
                config: self.project.config.clone(),
                runs_root: sandbox.join("runs"),
                worktrees_root: sandbox.join("worktrees"),
                storage_path: sandbox.join("events.db"),
                user_root: self.project.user_root.clone(),
                workspace: yunta_api::Workspace {
                    dir: worktree.clone(),
                    ..self.project.workspace.clone()
                },
            },
            clock: Arc::clone(&self.clock),
            ids: Arc::clone(&self.ids),
            env: self.env.clone(),
            // One stream per process: a sandbox sees the same two
            // stages the invocation that made it does.
            interrupt: self.interrupt.shared(),
            rail_command: self.rail_command.clone(),
            binary: self.binary.clone(),
        }
    }

    /// The two stages this context stops in, as the API is handed them.
    pub fn stop(&self) -> yunta_api::Stop {
        self.interrupt.stages()
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
    pub fn adapters(&self) -> yunta_api::machine::Adapters {
        yunta_api::machine::real_adapters(&self.project.config)
    }
}
