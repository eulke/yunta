//! The sentences for what still reaches a command as an engine error: a
//! decision that failed below the refusal, and an artifact view the store
//! could not write. A refusal is said by `yunta_render::refusal`.

#[cfg(test)]
mod tests;

use yunta_core::run_dir::ARTIFACTS_DIR;
use yunta_engine::{ResolveGateError, RunError, StoreError};

/// What an artifact view that cannot be written says: the artifact it is of,
/// and the object or the store's failed step that stopped it.
pub(crate) fn project_view(name: &str, source: &StoreError) -> String {
    let why = match source {
        StoreError::Object(problem) => yunta_render::failure::object(problem),
        StoreError::Io { .. } => source.to_string(),
    };
    yunta_core::with_causes(
        format!("failed to write the `{ARTIFACTS_DIR}/` view of `{name}`: {why}"),
        source,
    )
}

/// Why a decision cannot be recorded, in the words of the command that
/// records it: what the run refuses, or what failed below.
pub(crate) fn resolve_gate(error: &ResolveGateError) -> String {
    match error {
        ResolveGateError::Refused(refusal) => yunta_render::refusal::gate(refusal),
        ResolveGateError::Storage(error) => error.to_string(),
        ResolveGateError::Documents(run) => documents(run),
    }
}

/// What a decision whose documents cannot be read says: the run's error that
/// says why, with the causes behind it.
fn documents(run: &RunError) -> String {
    let why = match run {
        RunError::Object(problem) => yunta_render::failure::object(problem),
        other => other.to_string(),
    };
    yunta_core::with_causes(
        format!("the documents the decision shows cannot be read: {why}"),
        run,
    )
}
