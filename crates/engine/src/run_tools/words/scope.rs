//! The sentences a request for scope reads: why the project settled it in
//! the call, and what a session does next while a request it made waits.

use yunta_core::expansion::Denial;
use yunta_core::protected::Protection;
use yunta_core::text::counted;

/// The sentence a request's denial reads as to an agent: a reached path is
/// named with the reason it is protected, and a legacy denial reads back as
/// the text it was written with.
pub(crate) fn denial(refused: &Denial) -> String {
    match refused {
        Denial::ReachesProtected { reached } => format!(
            "it reaches what the task may never write: {}",
            reached
                .iter()
                .map(|reach| format!("{} ({})", reach.path, protected_as(reach.reason)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Denial::ModeDeny => "scope_expansion mode is deny (the default)".to_string(),
        Denial::NoCriterion => "rules mode requires a proposed_criterion".to_string(),
        Denial::PrecheckPasses => {
            "proposed criterion already passes — nothing to expand for".to_string()
        }
        Denial::CriterionCouldNotRun => {
            "proposed criterion could not run, so nothing judges the request".to_string()
        }
        Denial::TooManyFiles { touched, bound } => format!(
            "diff at the requested paths touches {}, over the {bound}-file bound",
            counted(*touched, "file")
        ),
        Denial::NothingLeft => {
            "no requested path is left to widen once the protected ones are out".to_string()
        }
        Denial::NotWidenable => "no person may widen this node's scope on this run".to_string(),
        Denial::Chosen { note, .. } => note
            .clone()
            .unwrap_or_else(|| "denied by a human at the gate".to_string()),
        Denial::Legacy { text } => text.clone(),
    }
}

/// Where a protected path's protection is written down, in the words a
/// request's denial names it with.
fn protected_as(reason: Protection) -> &'static str {
    match reason {
        Protection::ProjectDeny => "the project denies it to every run: permissions.paths.deny",
        Protection::AgentConfig => "an agent CLI's own configuration",
        Protection::AgentInstructions => "an instruction file, which only a person grants",
        Protection::ApprovedTest => "a test the task is held to",
        Protection::GitRegister => "the run's git register",
    }
}

/// What a session is told to do next while its request waits: a person
/// decides it when the attempt ends, and until then the work stays within the
/// scope it has; with no person to decide, the attempt is simply retried.
pub(crate) fn request_next(answered_by_a_person: bool, check_scope: &str) -> String {
    match answered_by_a_person {
        true => format!(
            "a person decides it when this attempt ends; until then, finish what you can within \
             the scope you have, without writing the paths you asked for; `{check_scope}` tells \
             you whether what you changed is within it"
        ),
        false => {
            "end this attempt; the work is attempted again once the request is decided".to_string()
        }
    }
}
