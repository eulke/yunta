//! `yunta review <run>`: reads the plan a run waits on a decision about, in full screen at
//! a terminal, and answers it there the way `resolve-gate` does. Off a terminal it prints the
//! plan and the commands that answer it.

use surco::components::{Fields, Section};
use surco::doc::Doc;
use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_core::shown::ShownDocument;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_render::command;
use yunta_render::page::{desk, Form, Page, Shown};

use crate::ask::{Console, Escape, Opens};
use crate::context::Context;
use crate::error::CliError;
use crate::output::Mode;
use crate::output::Outcome;
use crate::surface::Diagnostics;

use super::resolve_gate::{answering_commands, chosen_on, settled, Waiting};

/// Reads and answers the plan `run_id` waits on.
pub async fn review(run_id: &RunId, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let waiting = super::resolve_gate::asked_of(&ctx, run_id)
        .await?
        .read()
        .await?;
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
            waiting: &waiting,
        };
        crate::output::show(look, &page);
        return Ok(Outcome::Success);
    };
    let chosen = chosen_on(&console, &ctx, run_id, &waiting, Opens::ThePlan)?;
    settled(&ctx, run_id, chosen, None, None, output, look).await
}

/// The plan a run waits on, read in full the way a terminal reads it, with the commands
/// that answer the decision at its end: what a person off a terminal reads.
struct Review<'a> {
    plan: &'a ShownDocument,
    run_id: &'a RunId,
    waiting: &'a Waiting,
}

impl Page for Review<'_> {
    fn document(&self, look: &Look) -> Doc {
        let Review {
            plan,
            run_id,
            waiting,
        } = *self;
        let answers = answering_commands(run_id, &waiting.escalation)
            .into_iter()
            .fold(Fields::new(), |fields, (_, command)| {
                fields.push_command("", command)
            });
        let shown = Shown {
            document: plan,
            run: run_id,
            form: Form::Review,
        };
        shown.document(look).with(Section {
            lead: None,
            title: Line::new().push(Tone::Strong, "answer it with"),
            blocks: vec![answers.into()],
        })
    }
}
