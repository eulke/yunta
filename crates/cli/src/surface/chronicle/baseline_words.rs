//! What the chronicle says of the lineage's measurement: the suite, and
//! each command held beside it.

use yunta_core::events::BaselineOrigin;

use crate::render::Mark;

/// The suite, measured or inherited: whether any comparison can find a
/// regression against it.
pub(super) fn measured(origin: &BaselineOrigin, red: Option<i32>) -> (Option<Mark>, String) {
    let held = match origin {
        BaselineOrigin::Measured => "baseline measured".to_string(),
        BaselineOrigin::Inherited { run } => format!("baseline inherited from run {run}"),
    };
    match red {
        None => (None, held),
        Some(exit_code) => (
            Some(Mark::Caution),
            format!(
                "{held}, already red (exit {exit_code}): no comparison can find a regression, \
                 and no task is held to it"
            ),
        ),
    }
}

/// A command `baseline.held` names, measured beside the suite: whether it
/// holds the run's tasks.
pub(super) fn held(
    name: &yunta_core::CommandName,
    origin: &BaselineOrigin,
    red: Option<i32>,
) -> (Option<Mark>, String) {
    let held = match origin {
        BaselineOrigin::Measured => format!("`{name}` measured beside the baseline"),
        BaselineOrigin::Inherited { run } => format!("`{name}` inherited from run {run}"),
    };
    match red {
        None => (None, format!("{held}: every task is held to it")),
        Some(exit_code) => (
            Some(Mark::Caution),
            format!("{held}, already red (exit {exit_code}): no task is held to it"),
        ),
    }
}
