//! See [`super`]. The acts a run offers that a key on its stage takes: each step
//! `RunFrame::actions` offers that a key here is bound to, and answering what a run
//! waits on where it stopped on a menu of its own or on a round of questions, the step
//! `resolve-gate` takes with no option. Stopping and closing ask a confirmation first; going on acts at once; the
//! child a run waits on opens over it, and its decision and the plan it is on open in
//! the decision stage. The receipt a finished run offers, the verification a broken
//! one does, and the page for a browser every run offers open in the evidence stage;
//! what it cost and its graph, which every run offers too, in the stats and the graph
//! stages.

use yunta_core::events::GateWaitingPayload;
use yunta_core::step::{Choice, NextStep};
use yunta_render::command;
use yunta_replay::frame::RunFrame;

use super::super::Want;
use crate::dialog::Confirmation;
use crate::Operation;

/// An act a run offers here: the key it is bound to, the step it takes, and what it
/// does, in render's words.
pub(super) struct Offered {
    pub(super) key: char,
    pub(super) step: NextStep,
    pub(super) does: &'static str,
}

/// What the run framed as `frame` offers here: answering what it waits on where it
/// stopped on a menu of its own or on a round of questions (`answerable`), then each
/// act its standing and `decision`, the decision its page says it stopped on, offer that
/// a key here takes, then what every run offers: its page for a browser, what it cost
/// and its graph.
pub(super) fn offered(
    frame: &RunFrame,
    decision: Option<&GateWaitingPayload>,
    answerable: bool,
) -> Vec<Offered> {
    let answering = answerable.then(|| NextStep::ResolveGate {
        run: frame.run_id.clone(),
        choice: Choice::Asked,
    });
    let answering = answering.map(|step| Offered {
        key: 'a',
        does: command::does(&step),
        step,
    });
    let actions = frame.actions(decision).into_iter().filter_map(|action| {
        let key = bound(&action.step)?;
        Some(Offered {
            key,
            does: command::effect(action.effect),
            step: action.step,
        })
    });
    let run = &frame.run_id;
    let always = [
        ('o', NextStep::Open { run: run.clone() }),
        ('s', NextStep::Stats { run: run.clone() }),
        ('g', NextStep::RunGraph { run: run.clone() }),
    ]
    .map(|(key, step)| Offered {
        key,
        does: command::does(&step),
        step,
    });
    answering.into_iter().chain(actions).chain(always).collect()
}

/// The key the act taking `step` is bound to here; none for an act another stage
/// takes. [`taken`] says what each bound one asks for.
fn bound(step: &NextStep) -> Option<char> {
    match step {
        NextStep::Cancel { .. } => Some('c'),
        NextStep::Resume { .. } => Some('r'),
        NextStep::Close { .. } => Some('x'),
        NextStep::Status { .. } => Some('w'),
        NextStep::Review { .. } => Some('p'),
        NextStep::ResolveGate { .. } => Some('a'),
        NextStep::Receipt { .. } => Some('e'),
        NextStep::Verify { .. } => Some('v'),
        _ => None,
    }
}

/// What taking `step` asks for: stopping and closing after a confirmation, going on at
/// once, and any other the stage it opens, opened over this one.
pub(super) fn taken(step: NextStep) -> Vec<Want> {
    match step {
        NextStep::Cancel { run } => {
            let asks = yunta_render::screen::cancelling(run.handle());
            let same = vec![NextStep::Cancel { run: run.clone() }];
            vec![Want::Confirm(Confirmation::new(
                asks,
                same,
                Operation::Cancel { run },
            ))]
        }
        NextStep::Resume { run } => vec![Want::Work(Operation::Resume { run })],
        NextStep::Close { run } => {
            let asks = yunta_render::screen::closing(&[run.handle()]);
            let same = vec![NextStep::Close { run: run.clone() }];
            vec![Want::Confirm(Confirmation::new(
                asks,
                same,
                Operation::Close { runs: vec![run] },
            ))]
        }
        other => Want::taking(&other),
    }
}
