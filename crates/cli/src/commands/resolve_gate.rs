//! `yunta resolve-gate <run_id> <option>`: answers a paused run's gate
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
use crate::render::RunExit;
use crate::surface::Diagnostics;

/// Records the decision and hands the run back, returning the sentence
/// that says so.
///
/// The one path a gate is answered through, whichever door asked: the
/// command below prints this sentence on stdout, and the control
/// plane's `resolve_gate` tool returns it in a tool result. Two doors,
/// one decision, one wording — and one place that knows a decision
/// recorded is not a decision undone when the hand-off fails.
pub(crate) async fn resolve(
    ctx: &Context,
    run_id: &RunId,
    option: &OptionId,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
) -> Result<String, CliError> {
    let open = ctx.open_run(run_id).await?;
    let (run_dir, manifest) = (open.run_dir, open.manifest.doc);
    let storage = ctx.async_storage().await?;

    yunta_engine::resolve_gate(
        &manifest,
        &storage,
        run_id,
        &ctx.clock,
        HumanChoice {
            option: option.clone(),
            by: crate::identity::responder(resolved_by),
            free_text: free_text.map(str::to_string),
        },
    )
    .await
    .map_err(|refusal| CliError::gate_refused(run_id, refusal))?;

    super::spawn_detached_resume(&run_dir, run_id.as_str(), &ctx.cwd)
        .await
        .map_err(|source| CliError::GateRecordedNotResumed {
            source: super::DetachedResumeError::new(run_id, source),
        })?;
    Ok(format!(
        "run {run_id}: resolved `{option}`, driving forward independently"
    ))
}

/// Answers `run_id`'s decision with `option`, or — with none — with
/// the option a person picks off the run's own menu on this terminal.
pub async fn resolve_gate(
    run_id: &RunId,
    option: Option<&OptionId>,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let choice = match option {
        Some(option) => HumanChoice {
            option: option.clone(),
            by: crate::identity::responder(resolved_by),
            free_text: free_text.map(str::to_string),
        },
        None => match chosen(&ctx, run_id).await? {
            Chosen::Answered(choice) => HumanChoice {
                by: resolved_by.cloned().unwrap_or(choice.by),
                free_text: choice.free_text.or_else(|| free_text.map(str::to_string)),
                ..choice
            },
            Chosen::Not(outcome) => return Ok(outcome),
        },
    };
    println!(
        "{}",
        resolve(
            &ctx,
            run_id,
            &choice.option,
            Some(&choice.by),
            choice.free_text.as_deref()
        )
        .await?
    );
    Ok(Outcome::Success)
}

/// What the person at the menu did.
enum Chosen {
    Answered(HumanChoice),
    /// Nothing was recorded, and the invocation ends with this.
    Not(Outcome),
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
    let Some((_, escalation)) = yunta_engine::current_escalation(&open.manifest.doc, &state) else {
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
    let shown =
        yunta_engine::shown_documents(&open.run_dir, escalation.shows(), &state.tasks).await?;
    match crate::ask::decide(&console, &escalation, &shown) {
        Ok(choice) => Ok(Chosen::Answered(choice)),
        Err(NoAnswer::Declined) => {
            println!("run {called}: nothing recorded — it still waits on the decision");
            Ok(Chosen::Not(Outcome::Success))
        }
        Err(NoAnswer::Interrupted) => Ok(Chosen::Not(RunExit::Interrupted.outcome())),
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
