//! Where the process enters: the one place below `main` that reads it. The
//! arguments, the environment, the directory, this binary's own path and the
//! interrupt are read here once, and every command takes them as the
//! [`Workspace`] and the [`Stop`](yunta_api::Stop) the API is handed.
//!
//! Nothing else reads the process: a command that needs the directory, the
//! clock or a variable takes it from the shell, so two commands of one
//! invocation, or a command and the API under it, cannot read two values.

use std::ffi::OsString;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::signal::unix::{signal, SignalKind};

use yunta_api::Workspace;
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
        // The process's own SIGINT, installed here, synchronously, so the
        // handler exists the moment the shell does: a signal that arrives
        // while a project still resolves is one this invocation catches, not
        // one that kills it halfway through.
        let interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .map(Interrupt::driven_by)
            .map_err(|source| {
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

    /// What the API is handed: where this process works and its boundaries.
    pub(crate) fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// The two stages this invocation stops in, as the API is handed them.
    pub(crate) fn stop(&self) -> yunta_api::Stop {
        self.interrupt.stages()
    }

    /// What asks this process to end while it holds a terminal of its own, where a typed
    /// Ctrl-C is a key: an interrupt sent from outside, a terminate, or the terminal hanging
    /// up, whichever comes first. Installed when asked for, so a command that holds no
    /// terminal ends on a terminate or a hang-up as any process does.
    pub(crate) fn asked_to_end(&self) -> Result<impl Future<Output = EndSignal>, CliError> {
        let listening = |kind: SignalKind| {
            signal(kind).map_err(|source| {
                CliError::io("install the handler that ends", "the application", source)
            })
        };
        let mut interrupt = listening(SignalKind::interrupt())?;
        let mut terminate = listening(SignalKind::terminate())?;
        let mut hang_up = listening(SignalKind::hangup())?;
        Ok(async move {
            tokio::select! {
                _ = interrupt.recv() => EndSignal::Interrupt,
                _ = terminate.recv() => EndSignal::Terminate,
                _ = hang_up.recv() => EndSignal::HangUp,
            }
        })
    }
}

/// A signal that asks the process to end: SIGINT, SIGTERM or SIGHUP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndSignal {
    Interrupt,
    Terminate,
    HangUp,
}
