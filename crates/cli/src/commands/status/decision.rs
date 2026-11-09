//! What a run waits on, read off its directory with the engine: the decision
//! a parked run waits on and the documents its gate shows. The documents
//! that carry them are `yunta_render::document`'s.

use yunta_core::Manifest;
use yunta_replay::RunState;

use crate::error::CliError;

/// The decision a run waits on, rebuilt from the run's derived state and
/// the documents its gate shows — the one reading every surface takes.
pub(crate) async fn decision_of(
    manifest: &Manifest,
    run_dir: &std::path::Path,
    state: &RunState,
) -> Result<
    Option<(
        yunta_core::events::DecisionOn,
        yunta_core::events::Escalation,
    )>,
    CliError,
> {
    Ok(yunta_engine::current_escalation(manifest, run_dir, state).await?)
}

/// The documents `escalation` shows, as the run born with `manifest`,
/// whose derived state is `state`, holds them.
pub(crate) async fn shown_of(
    manifest: &yunta_core::Manifest,
    run_dir: &std::path::Path,
    escalation: &yunta_core::events::GateWaitingPayload,
    state: &RunState,
) -> Result<Vec<yunta_core::shown::ShownDocument>, CliError> {
    Ok(
        yunta_engine::shown_documents(run_dir, escalation.shows(), state, manifest.request())
            .await?,
    )
}
