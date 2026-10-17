//! The request file: what a session's request writes into its checkout, what
//! the engine reads back to decide it, and what leaves the checkout once it is
//! decided. A request is never decided from memory: the file is the record.

use std::path::Path;

use thiserror::Error;
use yunta_core::durable::{self, Durability};

use super::{ScopeExpansionError, ScopeExpansionRequest, SCOPE_EXPANSION_REQUEST_FILE};

/// Why a request could not be written into the request file.
#[derive(Debug, Error)]
pub(crate) enum RequestError {
    #[error("failed to read `{path}`: {detail}")]
    Read { path: String, detail: String },
    #[error("`{path}` does not parse as a scope expansion request: {detail}")]
    Malformed { path: String, detail: String },
    #[error("failed to write `{path}`: {detail}")]
    Write { path: String, detail: String },
    /// One file holds one criterion, so a request under another is refused.
    #[error("a request under criterion `{pending}` is already waiting")]
    CriterionConflict { pending: String },
}

/// What a request did to the request file: created it, or joined the request
/// already waiting in it, adding the paths that the waiting one did not hold.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Recorded {
    Written,
    Amended { added: usize },
}

/// Reads the task's request file, if the agent wrote one this attempt. `None`,
/// no file, is the ordinary case, not an error. The file stays in place until
/// [`consume_request`] removes it.
pub async fn read_request(
    task_worktree: &Path,
) -> Result<Option<ScopeExpansionRequest>, ScopeExpansionError> {
    let path = task_worktree.join(SCOPE_EXPANSION_REQUEST_FILE);
    parse_file(&path).await.map_err(|unread| {
        let path = path.display().to_string();
        match unread {
            Unread::Read(detail) => ScopeExpansionError::Read { path, detail },
            Unread::Malformed(detail) => ScopeExpansionError::Malformed { path, detail },
        }
    })
}

/// Removes the task's request file once a decision has read it. A file that
/// is not there has nothing to remove.
pub async fn consume_request(task_worktree: &Path) -> Result<(), ScopeExpansionError> {
    let path = task_worktree.join(SCOPE_EXPANSION_REQUEST_FILE);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ScopeExpansionError::Io {
            action: format!(
                "remove consumed scope expansion request `{}`",
                path.display()
            ),
            source,
        }),
    }
}

/// Why the request file could not be read back.
enum Unread {
    Read(String),
    Malformed(String),
}

/// The request in the file at `path`, or `None` when there is no file.
async fn parse_file(path: &Path) -> Result<Option<ScopeExpansionRequest>, Unread> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Unread::Read(source.to_string())),
    };
    yunta_core::yaml::parse_bytes(&bytes)
        .map(Some)
        .map_err(|source| Unread::Malformed(source.to_string()))
}

/// Writes `request` into the checkout's request file. `guard` is held through
/// the read and the write, so the requests of one run never interleave: with
/// no request waiting the file is created; otherwise the request's paths join
/// the waiting ones, and a request under another criterion is refused.
pub(crate) async fn write_request(
    checkout: &Path,
    request: ScopeExpansionRequest,
    guard: &tokio::sync::Mutex<()>,
) -> Result<Recorded, RequestError> {
    let _held = guard.lock().await;
    let path = checkout.join(SCOPE_EXPANSION_REQUEST_FILE);
    let read = parse_file(&path).await.map_err(|unread| {
        let path = path.display().to_string();
        match unread {
            Unread::Read(detail) => RequestError::Read { path, detail },
            Unread::Malformed(detail) => RequestError::Malformed { path, detail },
        }
    })?;
    let Some(mut waiting) = read else {
        write_file(&path, &request, true)?;
        return Ok(Recorded::Written);
    };
    if let (Some(pending), Some(asked)) = (criterion_of(&waiting), criterion_of(&request)) {
        if pending != asked {
            return Err(RequestError::CriterionConflict {
                pending: pending.to_string(),
            });
        }
    }
    // A request that names no criterion joins any file; one that names a
    // criterion gives it to a file that has none.
    if criterion_of(&waiting).is_none() {
        waiting.proposed_criterion = request.proposed_criterion.clone();
        waiting.evidence = request.evidence.clone();
    }
    let before = waiting.paths.len();
    for glob in request.paths {
        if !waiting.paths.contains(&glob) {
            waiting.paths.push(glob);
        }
    }
    write_file(&path, &waiting, false)?;
    Ok(Recorded::Amended {
        added: waiting.paths.len() - before,
    })
}

/// Writes `request` to `path` whole: created when `create` is set, replacing
/// the request already there otherwise.
fn write_file(
    path: &Path,
    request: &ScopeExpansionRequest,
    create: bool,
) -> Result<(), RequestError> {
    let failed = |detail: String| RequestError::Write {
        path: path.display().to_string(),
        detail,
    };
    let text = yunta_core::yaml::to_string(request).map_err(|source| failed(source.to_string()))?;
    let bytes = text.as_bytes();
    // blocking: one small file, written whole or not at all.
    if !create {
        return durable::replace(path, bytes, Durability::Atomic)
            .map_err(|source| failed(source.to_string()));
    }
    // A create answers `Ok(false)` when the file is already there: nothing is
    // overwritten, and the request is refused.
    match durable::create_new(path, bytes, Durability::Atomic) {
        Ok(true) => Ok(()),
        Ok(false) => Err(failed(
            "the file appeared before it was created".to_string(),
        )),
        Err(source) => Err(failed(source.to_string())),
    }
}

/// The criterion a request is decided under: the command it proposes, or the
/// command it cites as its evidence.
pub(crate) fn criterion_of(request: &ScopeExpansionRequest) -> Option<&str> {
    request
        .proposed_criterion
        .as_ref()
        .map(|proposed| proposed.cmd.as_str())
        .or(request.evidence.as_deref())
}
