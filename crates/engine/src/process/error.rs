//! How a governed command can fail, and what it had written by then.

use thiserror::Error;
use tokio::task::JoinError;
use yunta_core::process::group::GroupError;

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("failed to spawn `{command}`")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`{command}` spawned without a pid")]
    NoPid { command: String },
    #[error("failed to wait for `{command}`")]
    Wait {
        command: String,
        #[source]
        source: std::io::Error,
        output: Box<CapturedOutput>,
    },
    #[error("failed to observe the child process for `{command}`")]
    Observe {
        command: String,
        #[source]
        source: std::io::Error,
        output: Box<CapturedOutput>,
    },
    #[error("failed to kill the process group of `{command}`: {source}")]
    Kill {
        command: String,
        #[source]
        source: Box<GroupError>,
        output: Box<CapturedOutput>,
    },
    #[error("failed to read the {} of `{command}`", stream.as_str())]
    Read {
        command: String,
        stream: PipeKind,
        #[source]
        source: std::io::Error,
        output: Box<CapturedOutput>,
    },
    #[error("the task reading the {} of `{command}` failed", stream.as_str())]
    ReadTask {
        command: String,
        stream: PipeKind,
        #[source]
        source: JoinError,
        output: Box<CapturedOutput>,
    },
}

/// Output already captured when supervision failed. Kept behind a box in
/// [`SpawnError`] so carrying diagnostics does not inflate every run error.
#[derive(Debug)]
pub struct CapturedOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(super) fn captured_output(stdout: Vec<u8>, stderr: Vec<u8>) -> Box<CapturedOutput> {
    Box::new(CapturedOutput { stdout, stderr })
}

#[derive(Debug, Clone, Copy)]
pub enum PipeKind {
    Stdin,
    Stdout,
    Stderr,
}
impl PipeKind {
    /// The stream's name, as a reader of a command's output knows it.
    pub fn as_str(self) -> &'static str {
        match self {
            PipeKind::Stdin => "stdin",
            PipeKind::Stdout => "stdout",
            PipeKind::Stderr => "stderr",
        }
    }
}
