//! A run's socket: where it lives, binding it, serving its readers, and taking
//! it away.
//!
//! The socket is `scratch/live.sock` when that path fits what a socket's
//! address holds, and a file named by the run in the user's own directory
//! under `/tmp` otherwise. `/tmp` and not `$TMPDIR`: the path stays short on
//! every host whatever environment the engine started in, and no reader
//! computes it, since the run's registry names it.
//!
//! Only the engine's own user reaches it: the file is 0600, and a peer of
//! another uid is dropped as it connects. Each reader has a task of its own
//! that writes the recent past and then each frame, so a reader that stops
//! reading blocks only its own task, until the voices drop frames for it.
//! Dropping the [`Socket`] ends every task and removes the file. A crash
//! leaves the file behind, holding nothing, and the next engine for the run
//! replaces it.

use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};
use tokio_util::sync::CancellationToken;
use yunta_core::process::user;
use yunta_core::RunId;

use super::{Listening, Voices};

/// The longest path a socket binds at: the 104 bytes of the address macOS
/// and the BSDs give a socket, less its closing NUL.
#[cfg(not(target_os = "linux"))]
pub(crate) const PATH_MOST: usize = 103;

/// The longest path a socket binds at: the 108 bytes of the address Linux
/// gives a socket, less its closing NUL.
#[cfg(target_os = "linux")]
pub(crate) const PATH_MOST: usize = 107;

/// Where the user's own directory of sockets lives when a run's scratch path
/// is too long for one.
pub(crate) const SHORT_ROOT: &str = "/tmp";

/// Where the socket of the run `run` goes, for this process's user, given the
/// path it takes under the run's scratch.
pub fn socket_path(scratch: &Path, run: &RunId) -> Option<PathBuf> {
    place(scratch, run, Path::new(SHORT_ROOT), user::uid())
}

/// Where a run's socket goes: `scratch` when it fits, otherwise
/// `<run id>.sock` in `short_root/yunta-<uid>`, made private to `uid` when
/// nothing is there yet. `None` when that directory is not `uid`'s alone, or
/// the path still does not fit.
pub(crate) fn place(scratch: &Path, run: &RunId, short_root: &Path, uid: u32) -> Option<PathBuf> {
    if fits(scratch) {
        return Some(scratch.to_path_buf());
    }
    let dir = short_root.join(format!("yunta-{uid}"));
    let path = dir.join(format!("{run}.sock"));
    (fits(&path) && private_to(&dir, uid)).then_some(path)
}

fn fits(path: &Path) -> bool {
    path.as_os_str().len() <= PATH_MOST
}

/// Whether `dir` is a directory of `uid`'s alone, which no other user can
/// enter or write: not a link, owned by `uid`, with no group or other bits.
/// Made so when nothing is there.
fn private_to(dir: &Path, uid: u32) -> bool {
    // blocking: one directory, made once by the first run that needs it.
    let made = std::fs::DirBuilder::new().mode(0o700).create(dir);
    if made
        .as_ref()
        .is_err_and(|error| error.kind() != io::ErrorKind::AlreadyExists)
    {
        return false;
    }
    // blocking: one stat of that directory.
    let Ok(found) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    found.file_type().is_dir() && found.uid() == uid && found.mode() & 0o077 == 0
}

/// A run's socket while the run's engine serves it.
pub struct Socket {
    path: PathBuf,
    stop: CancellationToken,
}

impl Socket {
    /// Binds the socket at `path`, mode 0600, and serves `voices` there to
    /// every reader of `uid`'s. A socket a crashed engine left at `path` is
    /// replaced; anything else there refuses the bind and is left alone.
    /// Must run inside the engine's runtime, which serves the readers.
    pub fn bind(path: &Path, voices: Arc<Voices>, uid: u32) -> io::Result<Socket> {
        clear_stale(path)?;
        let listener = UnixListener::bind(path)?;
        // blocking: one change of mode, on the file just bound.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let stop = CancellationToken::new();
        tokio::spawn(accept(listener, voices, uid, stop.clone()));
        Ok(Socket {
            path: path.to_path_buf(),
            stop,
        })
    }

    /// Where the socket is bound.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Removes a socket left at `path`; refuses anything else that is there.
fn clear_stale(path: &Path) -> io::Result<()> {
    // blocking: one stat of where the socket goes.
    match std::fs::symlink_metadata(path) {
        // blocking: one unlink of a socket nobody serves.
        Ok(found) if found.file_type().is_socket() => std::fs::remove_file(path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "{} is not a socket, so a run's socket cannot go there",
                path.display()
            ),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Admits each reader of `uid`'s until `stop`. A failed accept ends the
/// serving: the run goes on, and no new reader connects.
async fn accept(listener: UnixListener, voices: Arc<Voices>, uid: u32, stop: CancellationToken) {
    loop {
        let accepted = tokio::select! {
            () = stop.cancelled() => return,
            accepted = listener.accept() => accepted,
        };
        let stream = match accepted {
            Ok((stream, _)) => stream,
            Err(error) => {
                tracing::warn!(%error, "a run's socket stopped accepting readers");
                return;
            }
        };
        let peer = stream.peer_cred().ok().map(|cred| cred.uid());
        if admits(peer, uid) {
            tokio::spawn(serve(stream, voices.listen(), stop.clone()));
        }
    }
}

/// Whether a peer is the engine's own user. A peer the system cannot name is
/// not.
pub(crate) fn admits(peer: Option<u32>, own: u32) -> bool {
    peer == Some(own)
}

/// Writes what `listening` hands, a line each, until the reader goes, the
/// voices end, or `stop`.
async fn serve(mut stream: UnixStream, mut listening: Listening, stop: CancellationToken) {
    loop {
        let heard = tokio::select! {
            () = stop.cancelled() => return,
            heard = listening.next() => heard,
        };
        let Some(heard) = heard else {
            return;
        };
        let mut line = heard.line();
        line.push('\n');
        let written = tokio::select! {
            () = stop.cancelled() => return,
            written = stream.write_all(line.as_bytes()) => written,
        };
        if written.is_err() {
            return;
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.stop.cancel();
        // blocking: one unlink, as the run lets go of its socket.
        if let Err(error) = std::fs::remove_file(&self.path) {
            tracing::warn!(path = %self.path.display(), %error, "failed to remove a run's socket");
        }
    }
}

#[cfg(test)]
mod tests;
