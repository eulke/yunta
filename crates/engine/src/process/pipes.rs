use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::ChildStdin;
use tokio::task::JoinError;
use yunta_core::process::group::GroupError;

use super::{Capture, PipeKind};

pub(super) enum PipeFailure {
    Io(PipeKind, std::io::Error),
    Join(PipeKind, JoinError),
    Group(GroupError),
}

#[derive(Clone, Default)]
pub(super) struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    pub(super) fn snapshot(&self) -> Vec<u8> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn append(&self, bytes: &[u8]) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
    }
}

pub(super) fn stdio(capture: Capture) -> Stdio {
    match capture {
        Capture::Discard => Stdio::null(),
        Capture::Collect => Stdio::piped(),
    }
}

pub(super) async fn read_to_capture<R: AsyncRead + Unpin>(
    mut pipe: R,
    capture: Captured,
) -> std::io::Result<()> {
    let mut buf = [0_u8; 8192];
    loop {
        let read = pipe.read(&mut buf).await?;
        if read == 0 {
            return Ok(());
        }
        let chunk = buf.get(..read).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the subprocess reader returned more bytes than its buffer",
            )
        })?;
        capture.append(chunk);
    }
}

/// Writes all of `bytes` to the child and closes its stdin. A child that
/// closes its end before reading everything decided it had read enough:
/// that is its own business, not a failure of the pipe.
pub(super) async fn write_then_close(mut pipe: ChildStdin, bytes: &[u8]) -> std::io::Result<()> {
    let written = match pipe.write_all(bytes).await {
        Ok(()) => pipe.shutdown().await,
        Err(error) => Err(error),
    };
    match written {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}
