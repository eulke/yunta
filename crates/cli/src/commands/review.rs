//! `yunta review <run>`: reads the plan a run waits on a decision about, in full screen at
//! a terminal, and answers it there the way `resolve-gate` does. Off a terminal it prints the
//! plan and the commands that answer it.

use surco::components::{Fields, Section};
use surco::doc::Doc;
use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_core::RunId;
use yunta_engine::ShownDocument;
use yunta_render::shown::Form;

use crate::ask::{Console, Escape, Opens};
use crate::commands::advice;
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::output::Mode;
use crate::surface::Diagnostics;

use super::resolve_gate::{answering_commands, chosen_on, settled, Waiting};

/// Reads and answers the plan `run_id` waits on.
pub async fn review(run_id: &RunId, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let waiting = super::resolve_gate::asked_of(&ctx, run_id)
        .await?
        .read()
        .await?;
    let Some((plan, _)) = yunta_render::shown::plan_among(&waiting.shown) else {
        return Err(CliError::msg(format!(
            "run {} waits on a decision that shows no plan; `{}` says what it does show",
            run_id.handle(),
            advice::status(run_id.handle())
        )));
    };
    let Some(console) = Console::open(&Diagnostics::none(), Escape::LeavesWaiting).await else {
        crate::output::document(look, page(plan, run_id, &waiting));
        return Ok(Outcome::Success);
    };
    let chosen = chosen_on(&console, &ctx, run_id, &waiting, Opens::ThePlan)?;
    settled(&ctx, run_id, chosen, None, None, output, look).await
}

/// The plan a run waits on, read in full the way a terminal reads it, with the commands
/// that answer the decision at its end.
fn page(plan: &ShownDocument, run_id: &RunId, waiting: &Waiting) -> Doc {
    let answers = answering_commands(run_id, &waiting.escalation)
        .into_iter()
        .fold(Fields::new(), |fields, (_, command)| {
            fields.push_command("", command)
        });
    yunta_render::shown::document(plan, run_id, Form::Review, &advice::spell).with(Section {
        lead: None,
        title: Line::new().push(Tone::Strong, "answer it with"),
        blocks: vec![answers.into()],
    })
}
