//! `yunta resolve-gate <run> [<option>]`: answers a run's waiting gate
//! decision from a separate process — no live surface attached to the
//! run itself, exactly the shape `yunta mcp`'s own `resolve_gate` tool
//! needs. Appends **only the decision** to the log
//! (`yunta_engine::resolve_gate` — decision and consequence are a
//! separate pair, and this only ever writes the former) and hands the
//! run off to a detached `yunta resume`, which consumes the pre-seeded
//! decision through the engine's one existing consequence path —
//! retry, abort, promote and internal gates alike. Same detach
//! mechanism as `run --detach`: a control-plane operation never blocks
//! for the run's own duration.

use yunta_core::events::HumanChoice;
use yunta_core::{OptionId, Responder, RunId};

use crate::ask::{Console, Escape, NoAnswer};
use crate::commands::advice;
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::json::SCHEMA_VERSION;
use crate::output::{emit, Answer, Mode};
use crate::render::doc::{Block, Doc};
use crate::render::ink::Line;
use crate::render::{Look, RunExit};
use crate::surface::Diagnostics;

/// Records the decision and hands the run back, returning the sentence
/// that says so.
///
/// The one path a gate is answered through, whichever door asked: the
/// command answers with the same sentence, and the control plane's
/// `resolve_gate` tool returns it in a tool result. Two doors, one
/// decision, one wording — and one place that knows a decision recorded
/// is not a decision undone when the hand-off fails.
pub(crate) async fn resolve(
    ctx: &Context,
    run_id: &RunId,
    option: &OptionId,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
) -> Result<String, CliError> {
    record(ctx, run_id, option, resolved_by, free_text).await?;
    Ok(sentence(run_id, option))
}

/// Records the decision on the run's log and hands the run to a detached
/// resume that applies it.
async fn record(
    ctx: &Context,
    run_id: &RunId,
    option: &OptionId,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
) -> Result<(), CliError> {
    let open = ctx.open_run(run_id).await?;
    let (run_dir, manifest) = (open.run_dir, open.manifest.doc);
    let storage = ctx.async_storage().await?;
    let redactor = yunta_engine::run_redactor(&manifest, &yunta_core::ProcessSecrets)?;

    yunta_engine::resolve_gate(
        &manifest,
        &run_dir,
        &storage,
        run_id,
        &ctx.clock,
        HumanChoice {
            option: option.clone(),
            by: crate::identity::responder(resolved_by, ctx.env.user.as_deref()),
            free_text: free_text.map(str::to_string),
        },
        &redactor,
    )
    .await
    .map_err(|refusal| CliError::gate_refused(run_id, refusal))?;

    super::spawn_detached_resume(&run_dir, run_id.as_str(), &ctx.cwd)
        .await
        .map_err(|source| CliError::GateRecordedNotResumed {
            source: super::DetachedResumeError::new(run_id, source),
        })?;
    Ok(())
}

/// The sentence a recorded decision is answered with.
fn sentence(run_id: &RunId, option: &OptionId) -> String {
    format!("run {run_id}: resolved `{option}`, driving forward independently")
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
            "`--json` needs the option: the menu a decision waits on is for a terminal — \
             name the option, as in `yunta resolve-gate {run_id} <option> --json`"
        ))),
        _ => Ok(()),
    }
}

/// Answers `run_id`'s decision with `option`, or — with none — with
/// the option a person picks off the run's own menu on this terminal.
pub async fn resolve_gate(
    run_id: &RunId,
    option: Option<&OptionId>,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    needs_an_option(run_id, option, output)?;
    let ctx = Context::load()?;
    let choice = match option {
        Some(option) => HumanChoice {
            option: option.clone(),
            by: crate::identity::responder(resolved_by, ctx.env.user.as_deref()),
            free_text: free_text.map(str::to_string),
        },
        None => match chosen(&ctx, run_id).await? {
            Chosen::Answered(choice) => HumanChoice {
                by: resolved_by.cloned().unwrap_or(choice.by),
                free_text: choice.free_text.or_else(|| free_text.map(str::to_string)),
                ..choice
            },
            Chosen::Not(state) => return emit(output, &answer(run_id, state), look),
        },
    };
    record(
        &ctx,
        run_id,
        &choice.option,
        Some(&choice.by),
        choice.free_text.as_deref(),
    )
    .await?;
    emit(
        output,
        &answer(
            run_id,
            GateState::Resolved {
                option: choice.option,
            },
        ),
        look,
    )
}

/// What the person at the menu did.
enum Chosen {
    Answered(HumanChoice),
    /// Nothing was recorded, and the invocation ends with this.
    Not(GateState),
}

/// What a gate answer did, as a document carries it.
#[derive(serde::Serialize)]
pub(crate) struct ResolveGateAnswer {
    schema_version: u32,
    run_id: RunId,
    #[serde(flatten)]
    state: GateState,
}

/// How a gate answer ended.
#[derive(serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum GateState {
    /// The decision is recorded and the run is handed back.
    Resolved { option: OptionId },
    /// Nothing was recorded: the person at the menu chose none, and the run
    /// keeps waiting on its decision.
    Declined,
    /// Nothing was recorded: the menu was interrupted, and the exit code says so.
    Interrupted,
}

fn answer(run_id: &RunId, state: GateState) -> ResolveGateAnswer {
    ResolveGateAnswer {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.clone(),
        state,
    }
}

impl Answer for ResolveGateAnswer {
    fn document<'a>(&'a self, _look: &Look) -> Doc<'a> {
        let said = match &self.state {
            GateState::Resolved { option } => sentence(&self.run_id, option),
            GateState::Declined => format!(
                "run {}: nothing recorded — it still waits on the decision",
                self.run_id.handle()
            ),
            GateState::Interrupted => return Doc::new(),
        };
        Doc::new().with(Block::Lines(vec![Line::new().plain(said)]))
    }

    fn outcome(&self) -> Outcome {
        match &self.state {
            GateState::Interrupted => RunExit::Interrupted.outcome(),
            GateState::Resolved { .. } | GateState::Declined => Outcome::Success,
        }
    }
}

/// The option a person picks off the menu `run_id` waits on, put to them
/// on this terminal — the same prompt a run asks on when it stops.
///
/// Off a terminal there is nobody to put it to, and what is refused
/// lists the command that chooses each option, so the next invocation
/// is one copied line.
async fn chosen(ctx: &Context, run_id: &RunId) -> Result<Chosen, CliError> {
    let open = ctx.open_run(run_id).await?;
    let state = yunta_engine::derive(&open.events);
    let called = run_id.handle();
    let decision =
        yunta_engine::current_escalation(&open.manifest.doc, &open.run_dir, &state).await?;
    let Some((_, escalation)) = decision else {
        return Err(CliError::msg(format!(
            "run {called} waits on no decision — `{}` says where it stands",
            advice::status(called)
        )));
    };
    let escalation = escalation.into_payload();
    let Some(console) = Console::open(&Diagnostics::none(), Escape::LeavesWaiting).await else {
        let commands: Vec<String> = escalation
            .options()
            .iter()
            .map(|option| format!("  yunta resolve-gate {called} {}", option.id))
            .collect();
        return Err(CliError::msg(format!(
            "run {called} waits on a decision, and no option was given — choose one:\n{}",
            commands.join("\n")
        )));
    };
    let shown = yunta_engine::shown_documents(&open.run_dir, escalation.shows(), &state).await?;
    match crate::ask::decide(&console, &ctx.env, &escalation, Some((run_id, &shown))) {
        Ok(choice) => Ok(Chosen::Answered(choice)),
        Err(NoAnswer::Declined) => Ok(Chosen::Not(GateState::Declined)),
        Err(NoAnswer::Interrupted) => Ok(Chosen::Not(GateState::Interrupted)),
        Err(NoAnswer::OffMenu) => Err(CliError::msg(
            "the menu answered with an option it does not offer — nothing recorded",
        )),
        Err(NoAnswer::Unreadable(error)) => Err(CliError::msg(format!(
            "this terminal could not be read or drawn on ({error}) — nothing recorded"
        ))),
        Err(NoAnswer::Failed(why)) => Err(CliError::msg(format!(
            "the menu ended without an answer ({why}) — nothing recorded"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::{OptionId, RunId};

    use super::{answer, GateState};
    use crate::output::Answer;
    use crate::render::{draw, Look};

    #[test]
    fn each_gate_answer_keeps_the_words_it_has_always_had() {
        let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        let said = |state: GateState| {
            let reply = answer(&run, state);
            draw(reply.document(&Look::plain()), &Look::plain())
        };
        assert_eq!(
            said(GateState::Resolved {
                option: OptionId::from_static("retry"),
            }),
            "run 01AAAAAAAAAAAAAAAAAAAAAAAA: resolved `retry`, driving forward independently\n"
        );
        assert_eq!(
            said(GateState::Declined),
            "run AAAAAA: nothing recorded — it still waits on the decision\n"
        );
        assert_eq!(said(GateState::Interrupted), "");
    }
}
