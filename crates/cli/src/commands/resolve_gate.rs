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

use std::collections::BTreeMap;

use surco::components::{Gist, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use surco::words::Standing;
use yunta_core::events::{ChangesCountedPayload, GateWaitingPayload, HumanChoice};
use yunta_core::step::NextStep;
use yunta_core::{OptionId, Responder, RunId};
use yunta_engine::DecisionOn;
use yunta_engine::RunState;
use yunta_engine::ShownDocument;
use yunta_render::gate::Route;

use crate::ask::{Ask, Console, Escape, NoAnswer, Opens};
use crate::commands::advice;
use crate::context::{Context, Opened};
use crate::error::{CliError, Outcome};
use crate::json::SCHEMA_VERSION;
use crate::output::{emit, Answer, Mode};
use crate::render::RunExit;
use crate::surface::Diagnostics;

/// Records the decision and hands the run back, returning the sentence
/// that says so.
///
/// The one path a gate is answered through, whichever door asked: the
/// control plane's `resolve_gate` tool returns this sentence in a tool
/// result, and the command draws the same words as a page. Two doors, one
/// decision, one wording, and one place that knows a decision recorded is
/// not a decision undone when the hand-off fails.
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
    // Said here because the resume below would say it only to `detached.log`, with
    // the answer already on the log and the run reading as moving.
    let state = yunta_engine::derive(&open.events);
    super::resume::refuse_if_driven(run_id, &open.run_dir, &state).await?;
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

/// What a resume does with a recorded decision, in the words both doors
/// say: it applies it, and the menu it was chosen from says what that does.
const APPLIED: &str = "a resume in the background applies it";

/// What a recorded decision is said to be, at either door.
fn resolved_as(option: &OptionId) -> String {
    format!("resolved `{option}`")
}

/// The sentence a recorded decision is answered with.
fn sentence(run_id: &RunId, option: &OptionId) -> String {
    format!("run {run_id}: {}; {APPLIED}", resolved_as(option))
}

/// What a recorded decision is answered with: the option, and that a
/// resume in the background applies it.
fn resolved(run_id: &RunId, option: &OptionId) -> Doc {
    let handle = run_id.handle();
    Doc::new()
        .with(Headline::new(
            Mark::Done,
            format!("run {handle}"),
            resolved_as(option),
        ))
        .with(Gist::new(APPLIED))
        .with(Next {
            steps: vec![(advice::status(handle), "follows it")],
        })
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
            advice::resolve_gate(run_id.as_str())
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
    let picked = match option {
        Some(option) => Chosen::Answered(HumanChoice {
            option: option.clone(),
            by: crate::identity::responder(resolved_by, ctx.env.user.as_deref()),
            free_text: free_text.map(str::to_string),
        }),
        None => chosen(&ctx, run_id).await?,
    };
    settled(&ctx, run_id, picked, resolved_by, free_text, output, look).await
}

/// What a person at the menu did, carried through: recorded and handed back, or the page
/// that says nothing was.
pub(super) async fn settled(
    ctx: &Context,
    run_id: &RunId,
    chosen: Chosen,
    by: Option<&Responder>,
    free_text: Option<&str>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let choice = match chosen {
        Chosen::Answered(choice) => HumanChoice {
            by: by.cloned().unwrap_or(choice.by),
            free_text: choice.free_text.or_else(|| free_text.map(str::to_string)),
            ..choice
        },
        Chosen::Not(state) => return emit(output, &answer(run_id, state), look),
    };
    record(
        ctx,
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
pub(super) enum Chosen {
    Answered(HumanChoice),
    /// Nothing was recorded, and the invocation ends with this.
    Not(GateState),
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

/// The decision `run_id` waits on, before anything it shows is read; a refusal that says
/// what serves when it waits on none.
pub(super) async fn asked_of(ctx: &Context, run_id: &RunId) -> Result<Asked, CliError> {
    let open = ctx.open_run(run_id).await?;
    let state = yunta_engine::derive(&open.events);
    let handle = run_id.handle();
    let decision =
        yunta_engine::current_escalation(&open.manifest.doc, &open.run_dir, &state).await?;
    let Some((on, escalation)) = decision else {
        return Err(CliError::msg(format!(
            "run {handle} waits on no decision; `{}` says where it stands",
            advice::status(handle)
        )));
    };
    Ok(Asked {
        open,
        state,
        on,
        escalation: escalation.into_payload(),
    })
}

/// A decision a run waits on, before what it shows is read.
pub(super) struct Asked {
    open: Opened,
    state: RunState,
    on: DecisionOn,
    pub(super) escalation: GateWaitingPayload,
}

impl Asked {
    /// The decision with the documents it shows and what the run changed.
    pub(super) async fn read(self) -> Result<Waiting, CliError> {
        let Asked {
            open,
            state,
            on,
            escalation,
        } = self;
        let asked = open.manifest.doc.request();
        let shown =
            yunta_engine::shown_documents(&open.run_dir, escalation.shows(), &state, asked).await?;
        let routes = yunta_render::gate::routes(
            &escalation,
            &open.manifest.doc.workflow,
            state.run.left_out(),
            on.node(),
        );
        Ok(Waiting {
            on,
            escalation,
            shown,
            routes,
            changed: state.current_change().cloned(),
            called: crate::render::called(&open.manifest.doc.workflow.name, state.run.mode()),
        })
    }
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
    fn document(&self, _look: &Look) -> Doc {
        let handle = self.run_id.handle();
        match &self.state {
            GateState::Resolved { option } => resolved(&self.run_id, option),
            GateState::Declined => Doc::new()
                .with(Headline::new(
                    Standing::NeedsYou.mark(),
                    format!("run {handle}"),
                    Standing::NeedsYou.word(),
                ))
                .with(Gist::new(
                    "nothing recorded: it still waits on the decision",
                ))
                .with(Next {
                    steps: vec![(advice::decide(handle), "puts the decision to you again")],
                }),
            GateState::Interrupted => Doc::new(),
        }
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
    let asked = asked_of(ctx, run_id).await?;
    let Some(console) = Console::open(&Diagnostics::none(), Escape::LeavesWaiting).await else {
        return Err(off_terminal(run_id, &asked.escalation));
    };
    let waiting = asked.read().await?;
    chosen_on(&console, ctx, run_id, &waiting, Opens::TheCard)
}

/// The option a person picks off the menu `run_id` waits on, put to them on `console`:
/// the decision's documents above its card, opened the way `opens` says.
pub(super) fn chosen_on(
    console: &Console,
    ctx: &Context,
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
    match crate::ask::decide(console, &ctx.env, ask) {
        Ok(choice) => Ok(Chosen::Answered(choice)),
        Err(NoAnswer::Declined) => Ok(Chosen::Not(GateState::Declined)),
        Err(NoAnswer::Interrupted) => Ok(Chosen::Not(GateState::Interrupted)),
        Err(NoAnswer::Unreadable(error)) => Err(CliError::msg(format!(
            "this terminal could not be read or drawn on ({error}): nothing recorded"
        ))),
        Err(NoAnswer::Failed(why)) => Err(CliError::msg(format!(
            "the menu ended without an answer ({why}): nothing recorded"
        ))),
    }
}

/// The command that chooses each option `escalation` offers for `run_id`, in the menu's
/// order, with the option it chooses.
pub(super) fn answering_commands(
    run_id: &RunId,
    escalation: &GateWaitingPayload,
) -> Vec<(OptionId, String)> {
    escalation
        .options()
        .iter()
        .map(|option| {
            let step = NextStep::ResolveGate {
                run: run_id.clone(),
                option: Some(option.id.clone()),
                text: option.asks().is_some(),
            };
            (option.id.clone(), advice::spell(&step))
        })
        .collect()
}

/// What `resolve-gate` says off a terminal, where nobody can be asked: that
/// the run waits on a decision, and the command that chooses each option it
/// offers, one to a line.
fn off_terminal(run_id: &RunId, escalation: &GateWaitingPayload) -> CliError {
    let commands: Vec<String> = answering_commands(run_id, escalation)
        .into_iter()
        .map(|(_, command)| format!("  {command}"))
        .collect();
    CliError::msg(format!(
        "run {} waits on a decision, and no option was given; choose one:\n{}",
        run_id.handle(),
        commands.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use yunta_core::{OptionId, RunId};

    use super::{answer, sentence, GateState};
    use crate::output::Answer;
    use crate::render::draw;
    use surco::look::Look;

    #[test]
    fn each_gate_answer_says_how_it_ended_and_what_to_type_next() {
        let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        let said = |state: GateState| {
            let reply = answer(&run, state);
            draw(reply.document(&Look::plain()), &Look::plain())
        };
        let resolved = said(GateState::Resolved {
            option: OptionId::from_static("retry"),
        });
        assert!(
            resolved.starts_with("+ run AAAAAA ; resolved `retry`\n"),
            "{resolved}"
        );
        assert!(resolved.contains("applies it"), "{resolved}");
        assert!(resolved.contains("yunta status AAAAAA"), "{resolved}");
        let declined = said(GateState::Declined);
        assert!(
            declined.starts_with("? run AAAAAA ; needs you\n"),
            "{declined}"
        );
        assert!(
            declined.contains("nothing recorded: it still waits on the decision."),
            "{declined}"
        );
        assert!(declined.contains("yunta resolve-gate AAAAAA"), "{declined}");
        assert_eq!(said(GateState::Interrupted), "");
    }

    /// Every recorded decision is answered alike: a resume in the background
    /// applies it, in the words the page and the MCP sentence share.
    #[test]
    fn every_answer_says_a_resume_applies_it() {
        let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        let abort = OptionId::from_static("abort");
        let reply = answer(
            &run,
            GateState::Resolved {
                option: abort.clone(),
            },
        );
        let page = draw(reply.document(&Look::plain()), &Look::plain());
        let said = page.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            said.contains("a resume in the background applies it"),
            "{page}"
        );
        assert!(!said.contains("stays open"), "{page}");
        assert!(!said.contains("drives the run on"), "{page}");
        assert_eq!(
            sentence(&run, &abort),
            "run 01AAAAAAAAAAAAAAAAAAAAAAAA: resolved `abort`; a resume in the background \
             applies it"
        );
        assert_eq!(
            sentence(&run, &OptionId::from_static("approve")),
            "run 01AAAAAAAAAAAAAAAAAAAAAAAA: resolved `approve`; a resume in the background \
             applies it"
        );
    }
}
