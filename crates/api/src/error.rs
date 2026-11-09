//! See [`crate`]. Why the API did not do what it was asked.

use std::fmt;

use yunta_render::refusal::Refused;

/// Why the API did not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// What Yunta refused: render says it, with the commands that go on.
    #[error(transparent)]
    Refused(#[from] Refused),
    /// What failed below the API: the store, the repository, the disk, an
    /// adapter, a workflow the catalog does not hold, said in its own words
    /// and its causes'.
    #[error(transparent)]
    Failed(#[from] Failure),
    /// The caller's stop.
    #[error("stopped")]
    Stopped,
}

/// A git the caller's stop cancelled is the stop, not a failure of anything;
/// any other git that did not answer failed below the API. One conversion, so
/// no operation has to remember to ask.
impl From<yunta_engine::git::GitError> for Error {
    fn from(git: yunta_engine::git::GitError) -> Self {
        match git.cancelled() {
            true => Error::Stopped,
            false => Failure::new(git).into(),
        }
    }
}

/// What the engine met reading or moving a run: an object the run's store
/// does not hold and an answer no decision takes are what Yunta refuses of the
/// run, the caller's stop is the stop, and anything else failed below the API.
impl From<yunta_engine::RunError> for Error {
    fn from(error: yunta_engine::RunError) -> Self {
        match error {
            yunta_engine::RunError::Object(problem) => Refused::Object(problem).into(),
            yunta_engine::RunError::RefusedAnswer {
                refused,
                summary,
                failure,
            } => Refused::SurfaceAnswer {
                refused,
                claim: summary,
                menu: failure,
            }
            .into(),
            yunta_engine::RunError::Cancelled => Error::Stopped,
            failed => Failure::new(failed).into(),
        }
    }
}

/// A manifest that could not be frozen: inputs the workflow does not take are
/// refused, a stop while git answers is the stop, and anything else failed
/// below the API.
impl From<yunta_engine::ManifestError> for Error {
    fn from(error: yunta_engine::ManifestError) -> Self {
        match error {
            yunta_engine::ManifestError::Inputs(problem) => Refused::Inputs(problem).into(),
            error if error.cancelled() => Error::Stopped,
            error => Failure::new(error).into(),
        }
    }
}

/// A tree a run works in that could not be prepared: a stop while git
/// answers is the stop, and anything else failed below the API.
impl From<yunta_engine::WorktreeError> for Error {
    fn from(error: yunta_engine::WorktreeError) -> Self {
        match error.cancelled() {
            true => Error::Stopped,
            false => Failure::new(error).into(),
        }
    }
}

/// What failed below the API, held as the error it was, so no interface
/// names an engine, store or adapter type and none branches on one: a
/// failure is said, in its own words and its causes'.
#[derive(Debug)]
pub struct Failure(Box<dyn std::error::Error + Send + Sync>);

impl Failure {
    /// `error`, which failed below the API.
    pub fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Failure(Box::new(error))
    }

    /// The failure as a person and an agent read it: its own sentence, then
    /// each cause behind it that the sentence does not already state.
    pub fn said(&self) -> String {
        yunta_core::describe(self)
    }
}

/// The failure's own sentence, as the error it holds says it.
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The causes are the held error's own, so a chain reads the same through
/// the API as it did below it.
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failure two levels deep, as the store says one: what it was doing,
    /// then the disk's own words.
    #[derive(Debug, thiserror::Error)]
    #[error("failed to read the log")]
    struct Outer {
        #[source]
        source: std::io::Error,
    }

    fn outer() -> Outer {
        Outer {
            source: std::io::Error::other("the disk is gone"),
        }
    }

    #[test]
    fn a_failure_says_its_chain_as_describe_says_it() {
        assert_eq!(Failure::new(outer()).said(), yunta_core::describe(&outer()));
        assert_eq!(
            Failure::new(outer()).said(),
            "failed to read the log: the disk is gone"
        );
    }

    #[test]
    fn a_failure_is_the_api_errors_failed_and_says_the_same() {
        let error: Error = Failure::new(outer()).into();
        assert!(matches!(error, Error::Failed(_)));
        assert_eq!(
            yunta_core::describe(&error),
            "failed to read the log: the disk is gone"
        );
    }
}
