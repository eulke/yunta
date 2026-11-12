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
//!
//! A run parked on the questions a node asked is answered the same way, with
//! no option to name: the round is put to the person at this terminal as the
//! run itself asks it, and the answers are recorded and handed on alike.

use surco::look::Look;
use yunta_api::{OpenedRun, Project, WaitingDecision, WaitingQuestions};
use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_core::step::{Choice, NextStep};
use yunta_core::{OptionId, Responder, RunId};
use yunta_render::command;
use yunta_render::command::Reader;
use yunta_render::document::GateAnswerDocument;
use yunta_render::page::{GateAnswered, Page, QuestionsAnswered, Resolution, RoundAnswer};
use yunta_render::prompt::Escape;
use yunta_render::refusal::Refused;
use yunta_ui::prompt::DecisionPut;

use crate::ask::{Console, NoAnswer};
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, show, Answer, Mode};
use crate::shell::Shell;
use crate::surface::Diagnostics;

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
        let open = project.run(run_id).await?;
        if let Some(waiting) = parked_round(&open).await? {
            return answered_round(&project, &open, waiting, resolved_by, output, look).await;
        }
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

/// The round `open` is parked on, or `None` where it waits on no questions and
/// what it waits on is a decision, or nothing.
async fn parked_round(open: &OpenedRun<'_>) -> Result<Option<WaitingQuestions>, CliError> {
    match open.waiting_questions().await {
        Ok(waiting) => Ok(Some(waiting)),
        Err(yunta_api::Error::Refused(Refused::NoDecision { .. })) => Ok(None),
        Err(other) => Err(other.into()),
    }
}

/// The round `open` is parked on, put to the person at this terminal as the run
/// asks it, recorded, and the run handed on; or the page that says nothing was.
/// Who the command line names stands over who answered at the round. Off a
/// terminal there is nobody to ask, and the refusal says where they are asked.
async fn answered_round(
    project: &Project,
    open: &OpenedRun<'_>,
    waiting: WaitingQuestions,
    by: Option<&Responder>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let WaitingQuestions { node, questions } = waiting;
    let replied = {
        let Some(console) = Console::open(&Diagnostics::none(), Escape::LeavesWaiting).await else {
            return Err(CliError::msg(QuestionsAnswered::off_terminal(
                &open.id, &node,
            )));
        };
        crate::ask::answer(&console, &project.workspace.env, &node, &questions)
    };
    let answer = match replied {
        Ok(mut reply) => {
            if let Some(by) = by {
                reply.responder = Some(by.clone());
            }
            open.answer(&node, reply).await?;
            RoundAnswer::Answered { node }
        }
        Err(NoAnswer::Declined) => RoundAnswer::Declined { node },
        Err(NoAnswer::Interrupted) => RoundAnswer::Interrupted,
        Err(NoAnswer::Unreadable(error)) => {
            return Err(CliError::msg(yunta_render::escalation::console_unreadable(
                &error.to_string(),
            )))
        }
        Err(NoAnswer::Failed(why)) => {
            return Err(CliError::msg(yunta_render::escalation::menu_unanswered(
                &why,
            )))
        }
    };
    if output == Mode::Human {
        let page = QuestionsAnswered {
            run_id: &open.id,
            answer: &answer,
        };
        show(look, &page);
    }
    Ok(match answer {
        RoundAnswer::Interrupted => Outcome::interrupted(),
        RoundAnswer::Answered { .. } | RoundAnswer::Declined { .. } => Outcome::Success,
    })
}

/// What a person at the menu did, carried through: recorded and handed back, or the page
/// that says nothing was. Who the command line names stands over who answered at the
/// menu, and the words typed at the menu over the command line's.
async fn settled(
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
    let waiting = project.run(run_id).await?.waiting_decision().await?;
    let Some(console) = Console::open(&Diagnostics::none(), Escape::LeavesWaiting).await else {
        return Err(off_terminal(run_id, &waiting.escalation));
    };
    chosen_on(&console, project, run_id, &waiting)
}

/// The option a person picks off the menu `run_id` waits on, put to them on `console`:
/// the decision's documents above its card, the plan's desk a key away.
fn chosen_on(
    console: &Console,
    project: &Project,
    run_id: &RunId,
    waiting: &WaitingDecision,
) -> Result<Chosen, CliError> {
    let put = DecisionPut::waiting(run_id, waiting);
    match crate::ask::decide(console, &project.workspace.env, &put) {
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
