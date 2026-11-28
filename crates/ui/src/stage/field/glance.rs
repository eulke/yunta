//! See [`super`]. The run the field points at, at a glance beside the field's list, as render
//! says it (`RunAtAGlance`): a run the field reads, with what a person can do about it from
//! the field, a key a row, from what the follower keeps of the run; or a run it cannot read,
//! with why. What a person can do stands under the list in the glance's stead where the
//! glance gives way.

use surco::components::{Region, Side, SideAt};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_api::{FieldReading, FollowedRun};
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_render::page::{FieldRow, GlanceAct, Page, RunAtAGlance};

use crate::dialog::palette::spelled;
use crate::prompt::Stroke;
use crate::FollowedRuns;

/// The run `pointed` of `framed` at a glance, drawn in `look`, beside the field's list at its
/// right, and what a person can do about it, which the list holds where the glance gives way:
/// none where the field names no such run.
pub(super) fn beside(
    followed: &FollowedRuns,
    framed: &FieldReading,
    pointed: &RunId,
    look: &Look,
) -> Option<Side> {
    let unread = framed.unreadable.iter().find(|run| run.run_id == *pointed);
    let (doc, instead) = match unread {
        Some(unread) => (RunAtAGlance::Unread(unread).document(look), None),
        None => read(followed, framed, pointed, look)?,
    };
    Some(Side {
        region: Region::new(doc),
        at: SideAt::Right,
        focused: false,
        instead: instead.into_iter().collect(),
    })
}

/// The run `pointed` of `framed`, which the field reads, at a glance drawn in `look`, and
/// what a person can do about it from the field, where it offers anything a key here takes.
fn read(
    followed: &FollowedRuns,
    framed: &FieldReading,
    pointed: &RunId,
    look: &Look,
) -> Option<(Doc, Option<Block>)> {
    let run = followed.field.as_ref()?.runs.get(pointed)?;
    let row = framed
        .rows
        .iter()
        .find(|row| row.frame.run_id == *pointed)?;
    let acts = acts_on(run, row);
    let manifest = run.manifest();
    let glance = RunAtAGlance::Read {
        row,
        base_branch: &manifest.base_branch,
        isolation: manifest.isolation,
        acts: &acts,
    };
    Some((glance.document(look), glance.what_you_can_do()))
}

/// What the field offers on `run`, whose row is `row`, each act with the key that takes it
/// here: opening it where the decision it waits on is read, where it waits on one; each
/// action its frame offers that a key here takes; and what it cost, where it asks at its
/// own terminal, where opening it answers nothing.
fn acts_on(run: &FollowedRun, row: &FieldRow) -> Vec<(String, GlanceAct)> {
    let decision = row
        .menu
        .then_some((Stroke::Enter, GlanceAct::ReadsTheDecision));
    let offered = run.actions().iter().filter_map(|action| {
        let key = key_of(&action.step)?;
        Some((key, GlanceAct::Offered(action.effect)))
    });
    let cost = row
        .asking()
        .then_some((Stroke::Insert('s'), GlanceAct::ShowsWhatItCost));
    decision
        .into_iter()
        .chain(offered)
        .chain(cost)
        .filter_map(|(key, act)| Some((spelled(key)?, act)))
        .collect()
}

/// The key the field takes the act that takes `step` on the run pointed at by: resuming it
/// with `r`, stopping it with `c`, closing it with `x`; none for a step no key here takes on
/// that run, as a way to another run. The one table the glance names a key by and a key acts
/// by.
pub(super) fn key_of(step: &NextStep) -> Option<Stroke> {
    match step {
        NextStep::Resume { .. } => Some(super::RESUMES),
        NextStep::Cancel { .. } => Some(super::STOPS),
        NextStep::Close { .. } => Some(super::CLOSES),
        _ => None,
    }
}
