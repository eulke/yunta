//! Where the process enters: the one place below `main` that reads it. The
//! arguments, the environment, the directory, this binary's own path and the
//! interrupt are read here once, and every command takes them as the
//! [`Workspace`] and the [`Stop`](yunta_api::Stop) the API is handed.
//!
//! Nothing else reads the process: a command that needs the directory, the
//! clock or a variable takes it from the shell, so two commands of one
//! invocation, or a command and the API under it, cannot read two values.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use yunta_api::Workspace;
use yunta_core::rail::RailCommand;
use yunta_core::{Env, ProcessSecrets, SystemClock, SystemIdSource};

use crate::error::CliError;
use crate::interrupt::Interrupt;

/// The arguments this process was started with, read once.
pub(crate) fn arguments() -> Vec<OsString> {
    std::env::args_os().collect()
}

/// The process environment, with every variable a built adapter reaches its
/// service by, read once: every color, glyph, width, terminal and log
/// decision reads it, and so does every command through its workspace. A
/// run leaves `subprocess_vars` empty: its nodes inherit this process's
/// environment unchanged.
pub(crate) fn environment() -> Env {
    Env::of_process(&yunta_api::machine::service_vars())
}

/// What a command reads of the process, read once: the workspace it hands the
/// API, and the interrupt that stops it.
pub(crate) struct Shell {
    workspace: Workspace,
    interrupt: Interrupt,
}

impl Shell {
    /// Reads the directory this process runs in and its own binary, and
    /// installs its interrupt, over the environment `main` read. The
    /// interrupt's stream exists the moment this returns, so a Ctrl-C that
    /// lands while a project still resolves is one this invocation catches.
    pub(crate) fn read(env: Env) -> Result<Self, CliError> {
        let dir = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
        let interrupt = Interrupt::ctrl_c().map_err(|source| {
            CliError::io("install the interrupt handler for", "this command", source)
        })?;
        Ok(Shell {
            workspace: Workspace {
                dir,
                env,
                clock: Arc::new(SystemClock),
                ids: Arc::new(SystemIdSource),
                secrets: Arc::new(ProcessSecrets),
                // A host that will not say where this binary lives falls back
                // to the name on `PATH`.
                binary: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("yunta")),
            },
            interrupt,
        })
    }

    /// A shell over `dir` with nothing to interrupt it and no process read:
    /// what a unit test of a command hands it.
    #[cfg(test)]
    pub(crate) fn for_test(dir: PathBuf) -> Self {
        Shell {
            workspace: Workspace {
                dir,
                env: Env::default(),
                clock: Arc::new(SystemClock),
                ids: Arc::new(SystemIdSource),
                secrets: Arc::new(ProcessSecrets),
                binary: PathBuf::from("yunta"),
            },
            interrupt: Interrupt::never(),
        }
    }

    /// The same workspace and the same two stages, owned: what a server that
    /// answers for the life of the process holds.
    pub(crate) fn shared(&self) -> Self {
        Shell {
            workspace: self.workspace.clone(),
            interrupt: self.interrupt.shared(),
        }
    }

    /// What the API is handed: where this process works and its boundaries.
    pub(crate) fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// The two stages this invocation stops in, as the API is handed them.
    pub(crate) fn stop(&self) -> yunta_api::Stop {
        self.interrupt.stages()
    }

    /// This invocation's interrupt, for a context or a server derived from
    /// the shell: one stream per process, two stages every one shares.
    pub(crate) fn interrupt(&self) -> Interrupt {
        self.interrupt.shared()
    }

    /// This binary, as the hook a CLI runs to ask the judge about one write.
    pub(crate) fn rail_command(&self) -> RailCommand {
        RailCommand::new(self.workspace.binary.clone())
    }
}
