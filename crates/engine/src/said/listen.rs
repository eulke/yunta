//! The reader of a run's socket, for a process that is not the run's engine.
//!
//! It reads a line at a time and hands each frame it can read. A line of a
//! version it does not know, or one that is not a frame, is skipped: what a
//! reader loses is words, never the reading. It writes nothing to the socket.

use std::io;
use std::path::Path;

use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::net::UnixStream;

use super::Heard;

/// A reader connected to a run's socket.
pub struct Hearing {
    lines: Lines<BufReader<UnixStream>>,
}

/// Connects to the run's socket at `path`, which the run's registry names.
pub async fn connect(path: &Path) -> io::Result<Hearing> {
    let stream = UnixStream::connect(path).await?;
    Ok(Hearing {
        lines: BufReader::new(stream).lines(),
    })
}

impl Hearing {
    /// The next frame the socket serves. `None` once the socket closes or its
    /// reading fails, which is the run's engine letting go of it.
    pub async fn next(&mut self) -> Option<Heard> {
        loop {
            let line = self.lines.next_line().await.ok()??;
            if let Ok(Some(heard)) = Heard::read(&line) {
                return Some(heard);
            }
        }
    }
}
