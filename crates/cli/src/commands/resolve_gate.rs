//! `yunta resolve-gate <run> [<option>]`: answers a run's waiting gate
//! decision from a separate process — no live surface attached to the
//! run itself, exactly the shape `yunta mcp`'s own `resolve_gate` tool
//! needs. The API appends **only the decision** to the log (decision and
//! consequence are a separate pair, and this only ever writes the former)
//! and hands the run off to a detached `yunta resume`, which consumes the
//! pre-seeded decision through the engine's one existing consequence path —
//! retry, abort, promote and internal gates alike. Same detach
//! mechanism as `run --detach`: a control-plane operation never blocks
//! for the run's own duration.

use std::collections::BTreeMap;

use surco::look::Look;
use yunta_core::events::{ChangesCountedPayload, DecisionOn, GateWaitingPayload, HumanChoice};
use yunta_core::shown::ShownDocument;
use yunta_core::step::{Choice, NextStep};
use yunta_core::{OptionId, Responder, RunId};
use yunta_render::command;
use yunta_render::command::Reader;
use yunta_render::escalation::Route;
use yunta_render::page::{GateAnswered, Page, Resolution};

use crate::ask::{Ask, Console, Escape, NoAnswer, Opens};
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;
use crate::surface::Diagnostics;
use yunta_api::{Project, WaitingDecision};
use yunta_render::document::GateAnswerDocument;

/// Records the decision on the run's log and hands the run to a detached
/// resume that applies it, through the API.
async fn record(
    project: &Project,
    run_id: &RunId,
    option: &OptionId,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
) -> Result<(), CliError> {
    let open = project.run(run_id).await?;
    let free_text = free_text.map(str::to_string);
    Ok(open.decide(option.clone(), resolved_by, free_text).await?)
}

/// Refuses a `--json` answer that names no option. The menu a decision
/// waits on is drawn on a terminal, which a `--json` reader does not have,
/// so the refusal is made ahead of reading the run or recording anything.
fn needs_an_option(
    run_id: &RunId,
    option: Option<&OptionId>,
    output: Mode,
) -> Result<(), CliError> {
    match option {
        None if output == Mode::Json => Err(CliError::msg(format!(
            "`--json` needs the option: the menu a decision waits on is for a terminal; \
             name the option, as in `{}`, and add `--json`",
            command::line_for(
                &NextStep::ResolveGate {
                    run: run_id.clone(),
                    choice: Choice::Blank,
                },
                Reader::Agent,
            )
        ))),
        _ => Ok(()),
    }
}

/// Answers `run_id`'s decision with `option`, or — with none — with
/// the option a person picks off the run's own menu on this terminal.
pub async fn resolve_gate(
    shell: &Shell,
    run_id: &RunId,
    option: Option<&OptionId>,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    needs_an_option(run_id, option, output)?;
    let project = shell.workspace().project()?;
    let Some(option) = option else {
        let chosen = chosen(&project, run_id).await?;
        return settled(
            &project,
            run_id,
            chosen,
            resolved_by,
            free_text,
            output,
            look,
        )
        .await;
    };
    recorded(
        &project,
        run_id,
        option,
        resolved_by,
        free_text,
        output,
        look,
    )
    .await
}

/// What a person at the menu did, carried through: recorded and handed back, or the page
/// that says nothing was. Who the command line names stands over who answered at the
/// menu, and the words typed at the menu over the command line's.
pub(super) async fn settled(
    project: &Project,
    run_id: &RunId,
    chosen: Chosen,
    by: Option<&Responder>,
    free_text: Option<&str>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let choice = match chosen {
        Chosen::Answered(choice) => choice,
        Chosen::Not(state) => return emit(output, &answer(run_id, state), look),
    };
    let by = by.unwrap_or(&choice.by);
    let free_text = choice.free_text.as_deref().or(free_text);
    recorded(
        project,
        run_id,
        &choice.option,
        Some(by),
        free_text,
        output,
        look,
    )
    .await
}

/// Records `option` on the decision `run_id` waits on, hands the run back, and
/// answers that the decision is resolved. Who decided is `by`, or whom the API
/// stamps when it names nobody.
async fn recorded(
    project: &Project,
    run_id: &RunId,
    option: &OptionId,
    by: Option<&Responder>,
    free_text: Option<&str>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    record(project, run_id, option, by, free_text).await?;
    let resolution = Resolution::Resolved {
        option: option.clone(),
    };
    emit(output, &answer(run_id, resolution), look)
}

/// What the person at the menu did.
pub(super) enum Chosen {
    Answered(HumanChoice),
    /// Nothing was recorded, and the invocation ends with this.
    Not(Resolution),
}

/// A decision a run waits on, with what it shows.
pub(super) struct Waiting {
    pub(super) on: DecisionOn,
    pub(super) escalation: GateWaitingPayload,
    pub(super) shown: Vec<ShownDocument>,
    routes: BTreeMap<OptionId, Route>,
    changed: Option<ChangesCountedPayload>,
    called: String,
}

/// The decision `run_id` waits on, with what it shows and what the run
/// changed, as the menu a person picks from is drawn: refused, by the API,
/// when the run waits on none.
pub(super) async fn waiting_of(project: &Project, run_id: &RunId) -> Result<Waiting, CliError> {
    let open = project.run(run_id).await?;
    let WaitingDecision {
        on,
        escalation,
        shown,
        state,
    } = open.waiting_decision().await?;
    let workflow = &open.manifest.doc.workflow;
    let routes =
        yunta_render::escalation::routes(&escalation, workflow, state.run.left_out(), on.node());
    Ok(Waiting {
        on,
        escalation,
        shown,
        routes,
        changed: state.current_change().cloned(),
        called: yunta_render::state::called(&workflow.name, state.run.mode()),
    })
}

/// What a gate answer did: the document a program reads, and the page a
/// person reads.
pub(crate) struct ResolveGateAnswer {
    run_id: RunId,
    resolution: Resolution,
}

fn answer(run_id: &RunId, resolution: Resolution) -> ResolveGateAnswer {
    ResolveGateAnswer {
        run_id: run_id.clone(),
        resolution,
    }
}

impl serde::Serialize for ResolveGateAnswer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let document = GateAnswerDocument {
            run_id: &self.run_id,
            resolution: &self.resolution,
        };
        serde::Serialize::serialize(&document, serializer)
    }
}

impl Answer for ResolveGateAnswer {
    fn page(&self) -> impl Page + '_ {
        GateAnswered {
            run_id: &self.run_id,
            resolution: &self.resolution,
        }
    }

    fn outcome(&self) -> Outcome {
        match &self.resolution {
            Resolution::Interrupted => crate::output::Outcome::interrupted(),
            Resolution::Resolved { .. } | Resolution::Declined => Outcome::Success,
        }
    }
}

/// The option a person picks off the menu `run_id` waits on, put to them
/// on this terminal — the same prompt a run asks on when it stops.
///
/// Off a terminal there is nobody to put it to, and what is refused
/// lists the command that chooses each option, so the next invocation
/// is one copied line.
async fn chosen(project: &Project, run_id: &RunId) -> Result<Chosen, CliError> {
    let waiting = waiting_of(project, run_id).await?;
    let Some(console) = Console::open(&Diagnostics::none(), Escape::LeavesWaiting).await else {
        return Err(off_terminal(run_id, &waiting.escalation));
    };
    chosen_on(&console, project, run_id, &waiting, Opens::TheCard)
}

/// The option a person picks off the menu `run_id` waits on, put to them on `console`:
/// the decision's documents above its card, opened the way `opens` says.
pub(super) fn chosen_on(
    console: &Console,
    project: &Project,
    run_id: &RunId,
    waiting: &Waiting,
    opens: Opens,
) -> Result<Chosen, CliError> {
    let ask = Ask {
        escalation: &waiting.escalation,
        shown: Some((run_id, &waiting.shown)),
        changed: waiting.changed.as_ref(),
        routes: waiting.routes.clone(),
        node: waiting.on.node(),
        called: Some(waiting.called.clone()),
        opens,
    };
    match crate::ask::decide(console, &project.workspace.env, ask) {
        Ok(choice) => Ok(Chosen::Answered(choice)),
        Err(NoAnswer::Declined) => Ok(Chosen::Not(Resolution::Declined)),
        Err(NoAnswer::Interrupted) => Ok(Chosen::Not(Resolution::Interrupted)),
        Err(NoAnswer::Unreadable(error)) => Err(CliError::msg(
            yunta_render::escalation::console_unreadable(&error.to_string()),
        )),
        Err(NoAnswer::Failed(why)) => Err(CliError::msg(
            yunta_render::escalation::menu_unanswered(&why),
        )),
    }
}

/// What `resolve-gate` says off a terminal, where nobody can be asked: that
/// the run waits on a decision, and the command that chooses each option it
/// offers, one to a line.
fn off_terminal(run_id: &RunId, escalation: &GateWaitingPayload) -> CliError {
    let commands: Vec<String> = command::answering(run_id, escalation)
        .into_iter()
        .map(|(_, command)| format!("  {command}"))
        .collect();
    CliError::msg(format!(
        "run {} waits on a decision, and no option was given; choose one:\n{}",
        run_id.handle(),
        commands.join("\n")
    ))
}
