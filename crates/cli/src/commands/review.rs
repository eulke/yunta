//! `yunta review <run>`: reads the plan a run waits on a decision about, in full screen at
//! a terminal, and answers it there the way `resolve-gate` does. Off a terminal it prints the
//! plan and the commands that answer it.

use surco::look::Look;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_render::command;
use yunta_render::page::{desk, Review};

use crate::ask::{Console, Escape, Opens};
use crate::context::Context;
use crate::error::CliError;
use crate::output::Mode;
use crate::output::Outcome;
use crate::shell::Shell;
use crate::surface::Diagnostics;

use super::resolve_gate::{chosen_on, settled};

/// Reads and answers the plan `run_id` waits on.
pub async fn review(
    shell: &Shell,
    run_id: &RunId,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::of(shell)?;
    let waiting = super::resolve_gate::waiting_of(&ctx, run_id).await?;
    let Some((plan, _)) = desk::plan_among(&waiting.shown) else {
        return Err(CliError::msg(format!(
            "run {} waits on a decision that shows no plan; `{}` says what it does show",
            run_id.handle(),
            command::line(&NextStep::status(run_id))
        )));
    };
    let Some(console) = Console::open(&Diagnostics::none(), Escape::LeavesWaiting).await else {
        let page = Review {
            plan,
            run_id,
            escalation: &waiting.escalation,
        };
        crate::output::show(look, &page);
        return Ok(Outcome::Success);
    };
    let chosen = chosen_on(&console, &ctx, run_id, &waiting, Opens::ThePlan)?;
    settled(&ctx, run_id, chosen, None, None, output, look).await
}
