//! The sentences a person reads for what the engine refuses, and for what a
//! command cannot resolve. An engine error holds its data and a code; the
//! sentence is written here, in the vocabulary of the command that reports
//! it, so the engine never names a command or a flag.

mod refusals;
mod wrapped;

#[cfg(test)]
mod tests;

pub(crate) use refusals::check;

use yunta_core::events::{Claim, FailureMenu, Refusal};
use yunta_core::inputs::InputsProblem;
use yunta_core::ARTIFACTS_DIR;
use yunta_engine::{CloseRunError, ResolveGateError, RunError, StoreError};

/// What the inputs a command was handed do not satisfy, in the words a
/// reader sees. An input the command was not given is also named by the flag
/// that would have given it.
pub(crate) fn inputs(problem: &InputsProblem) -> String {
    let said = yunta_render::inputs::problem(problem);
    match problem {
        InputsProblem::Missing { name } => format!("{said} — pass `--input {name}=...`"),
        _ => said,
    }
}

/// What a surface's answer was refused for: the refusal, and the claim the
/// answer was given for.
pub(crate) fn refused_answer(
    refused: &Refusal,
    claim: &Claim,
    menu: Option<&FailureMenu>,
) -> String {
    format!(
        "the human surface's answer was refused: {}, for: {}",
        refusal(refused),
        yunta_render::claim::sentence(claim, menu)
    )
}

/// Why an answer does not count as a decision on its escalation, in the words
/// a person reads.
fn refusal(refused: &Refusal) -> String {
    match refused {
        Refusal::OffMenu { chosen, offered } => {
            format!("`{chosen}` is not one of the options offered ({offered})")
        }
        Refusal::Unsaid { chosen, asks } => format!(
            "`{chosen}` asks \"{}\", and the answer says nothing",
            yunta_render::offers::asked(asks)
        ),
        Refusal::Withheld { withheld } => format!(
            "`{}` is withheld: {}",
            withheld.option,
            yunta_render::escalation::because(withheld)
        ),
    }
}

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
/// records it.
pub(crate) fn resolve_gate(error: &ResolveGateError) -> String {
    match error {
        ResolveGateError::NotPaused => "this run isn't parked at a pause — a live process may still \
             be driving it, or it already finished"
            .to_string(),
        ResolveGateError::NothingToResolve => "this run isn't currently waiting on a decision \
             `resolve_gate` can answer — it's paused for a reason with no menu of options (a failed \
             gate node, a budget cap, an external gate with no forge)"
            .to_string(),
        ResolveGateError::UnknownOption { chosen, declared } => {
            format!("option `{chosen}` isn't valid here — declared options: {declared}")
        }
        ResolveGateError::Unsaid { chosen, asks } => format!(
            "option `{chosen}` asks \"{}\" — say it with the choice",
            yunta_render::offers::asked(asks)
        ),
        ResolveGateError::Withheld { chosen, withheld } => format!(
            "option `{chosen}` is withheld: {}",
            yunta_render::escalation::because(withheld)
        ),
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

/// Why a run cannot be closed, in the words of the command that closes it.
pub(crate) fn close(error: &CloseRunError) -> String {
    match error {
        CloseRunError::AlreadyClosed => "the run is already closed".to_string(),
        CloseRunError::Driven => "an engine is driving the run".to_string(),
        CloseRunError::Moving => {
            "the run has not stopped, and nothing says the engine that drove it is gone".to_string()
        }
        CloseRunError::Storage(error) => error.to_string(),
        CloseRunError::Export(detail) => {
            format!("could not write the run's events.jsonl: {detail}")
        }
    }
}
