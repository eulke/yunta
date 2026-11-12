//! `yunta review <run>`: the plan a run waits on a decision about. At a terminal it opens the
//! application on the plan, over the run and the field, as a person walks there, and the
//! decision is answered there the way `resolve-gate` answers it. Off a terminal it prints the
//! plan and the commands that answer it. Wherever it shows the plan, its checks come first:
//! the run found, the decision it waits on read, a plan among what that decision shows.

use surco::look::Look;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_render::page::{desk, Review};
use yunta_render::refusal::Refused;
use yunta_ui::State;

use crate::application;
use crate::error::CliError;
use crate::output::Outcome;
use crate::shell::Shell;

/// The plan `run_id` waits on, opened in the application at a terminal, or printed.
pub async fn review(shell: &Shell, run_id: &RunId, look: &Look) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let waiting = project.run(run_id).await?.waiting_decision().await?;
    let Some((plan, _)) = desk::plan_among(&waiting.shown) else {
        let run = run_id.clone();
        return Err(Refused::NoPlan { run }.into());
    };
    if application::opens_here() {
        let reviewed = NextStep::Review {
            run: run_id.clone(),
        };
        let now = shell.workspace().clock.now();
        if let Some(opened) = State::opened_on(now, &reviewed) {
            if let Some(outcome) = application::at_this_terminal(shell, look, opened).await? {
                return Ok(outcome);
            }
        }
    }
    let page = Review {
        plan,
        run_id,
        escalation: &waiting.escalation,
    };
    crate::output::show(look, &page);
    Ok(Outcome::Success)
}
