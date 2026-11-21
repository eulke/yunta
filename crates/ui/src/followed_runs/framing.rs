//! See [`super`]. The frame a screen of the application stands in, composed once for every
//! stage that draws it: the tally at the trail's far end and the attention row over the
//! legend, read from the field the state keeps and from where the person stands.
//!
//! The tally counts the open runs of the field followed in each group that asks for a look,
//! as the field groups them: those that need the person, a run asked at its own terminal and
//! a run whose log broke among them, then those stalled. A field not read yet counts
//! nothing; one with no run in either group says that nothing needs the person.
//!
//! The attention row stands on every screen but where the run's own decision stands in
//! sight, as a round, a card or the desk: the decision is the row's there. On a run's
//! screen whose own decision is out of sight, the person leading it or a stage standing
//! over it, the row names that decision. Elsewhere, and on a run's screen the run leads,
//! whose pane shows what it waits on once read, the row names the run of the field that has
//! waited longest on the person among those answered here (`waiting_longest`, which the
//! window's title is chosen by too). A run asked at its own terminal is answered there
//! alone: it is counted, and never named. The run the row names is where `a` takes the
//! person, read by the same rule on every screen that draws the row ([`Attended`]): the run's
//! own decision, or another run, so the key never goes where the row does not say.

use chrono::{DateTime, Utc};
use surco::components::{Attention, Count, Tally};
use surco::words::Standing;
use yunta_core::RunId;
use yunta_render::page::{FieldGroup, FieldRow};
use yunta_render::watched;

use super::FollowedRuns;

/// The groups of the field a frame's tally counts the runs of, each by the word it says
/// them at, in the order it says them.
const TALLIED: [(FieldGroup, Standing); 2] = [
    (FieldGroup::NeedsYou, Standing::NeedsYou),
    (FieldGroup::Stalled, Standing::Stalled),
];

/// Where the person stands, as a screen's frame reads it: the run whose screen they are on,
/// if any, with where that run's own decision stands, and whether a line is typed there.
/// Not `FieldReach`, how far the field reaches.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Standpoint<'a> {
    /// The run whose screen the person is on, and where its own decision stands there; none
    /// on a screen that shows no run.
    pub(crate) run: Option<(&'a RunId, ItsDecision)>,
    /// Whether a line is typed where the person is: every key types there, `a` among them,
    /// so the attention row names no key.
    pub(crate) typed: bool,
}

/// Where `a` takes the person from a screen whose attention row names a run, by the run it
/// names: the run whose screen they are on, its own decision; or another run. Not the
/// attention row, which says where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Attended {
    /// The run's own decision, on the run's screen or a page over it.
    ItsOwn,
    /// Another run, whose screen takes this one's place.
    Another(RunId),
}

/// Where the run whose screen the person is on keeps its own decision, as the attention
/// row reads it. Not the scene, what the screen's main area stands as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItsDecision {
    /// In sight, as a round, a card or the desk standing in place: no attention row stands.
    InSight,
    /// The run leads its screen, whose pane shows what it waits on once read: the row names
    /// another run.
    InItsPane,
    /// Out of sight, the person leading the run's screen or a stage standing over it: the
    /// row names it where it waits on the person.
    OutOfSight,
}

impl FollowedRuns {
    /// The tally and the attention row of a screen the person reads from `at`, at `now`:
    /// neither before the field is read.
    pub(crate) fn framing(
        &self,
        at: Standpoint<'_>,
        now: DateTime<Utc>,
    ) -> (Tally, Option<Attention>) {
        let Some(field) = self.field.as_ref() else {
            return (Tally::Uncounted, None);
        };
        let rows = field.rows(now);
        let open: Vec<&FieldRow> = rows
            .iter()
            .filter(|row| row.group() != FieldGroup::Closed)
            .collect();
        let counts = TALLIED.into_iter().map(|(group, standing)| Count {
            standing,
            runs: open.iter().filter(|row| row.group() == group).count(),
        });
        let tally = Tally::Counted(counts.filter(|count| count.runs > 0).collect());
        let named = self.named_among(at, &open, now);
        let attention = named.map(|row| watched::needing(&row.frame, at.typed));
        (tally, attention)
    }

    /// Where `a` takes the person from a screen they read from `at`, at `now`: to the run its
    /// attention row names, the run's own decision where that is the run they stand on;
    /// nowhere where the row names none. Not [`FollowedRuns::framing`], which says it.
    pub(crate) fn attended(&self, at: Standpoint<'_>, now: DateTime<Utc>) -> Option<Attended> {
        let named = self.named_in_attention(at, now)?;
        Some(match at.run {
            Some((run, _)) if *run == named => Attended::ItsOwn,
            Some(_) | None => Attended::Another(named),
        })
    }

    /// The run the attention row of a screen the person reads from `at` names at `now`, where
    /// it names one.
    fn named_in_attention(&self, at: Standpoint<'_>, now: DateTime<Utc>) -> Option<RunId> {
        let rows = self.field.as_ref()?.rows(now);
        let open: Vec<&FieldRow> = rows
            .iter()
            .filter(|row| row.group() != FieldGroup::Closed)
            .collect();
        let named = self.named_among(at, &open, now);
        named.map(|row| row.frame.run_id)
    }

    /// The row of the run the attention row names among the field's `open` rows, read from
    /// `at` at `now`: none where the run's own decision stands in sight; the run's own where
    /// it is out of sight and needs the person; else the run that waited longest on the
    /// person, the run's own left out where its pane shows what it waits on.
    fn named_among(
        &self,
        at: Standpoint<'_>,
        open: &[&FieldRow],
        now: DateTime<Utc>,
    ) -> Option<FieldRow> {
        let longest = || waiting_longest(open.iter().copied()).cloned();
        match at.run {
            Some((_, ItsDecision::InSight)) => None,
            Some((run, ItsDecision::InItsPane)) => {
                let others = open.iter().copied().filter(|row| row.frame.run_id != *run);
                waiting_longest(others).cloned()
            }
            Some((run, ItsDecision::OutOfSight)) => self
                .row_of(run, open, now)
                .filter(answered_here)
                .or_else(longest),
            None => longest(),
        }
    }

    /// `run`'s row at `now`: the field's, among its `open` rows, where it holds it; the run
    /// in view's otherwise, where it is that run.
    fn row_of(&self, run: &RunId, open: &[&FieldRow], now: DateTime<Utc>) -> Option<FieldRow> {
        let field = open.iter().find(|row| row.frame.run_id == *run);
        let in_view = || {
            let in_view = self.in_view.as_deref();
            let held = in_view.filter(|in_view| in_view.run.run == *run);
            held.map(|in_view| in_view.run.row(now))
        };
        field.map(|row| (*row).clone()).or_else(in_view)
    }
}

/// Of `rows`, the run that has waited longest on the person among those answered here; none
/// where none is.
pub(crate) fn waiting_longest<'r>(
    rows: impl IntoIterator<Item = &'r FieldRow>,
) -> Option<&'r FieldRow> {
    rows.into_iter()
        .filter(|row| answered_here(row))
        .min_by(|a, b| FieldRow::by_wait(a, b))
}

/// Whether `row` is a run that needs the person and is answered here: not one whose engine
/// asks at its own terminal, where only an answer there moves it.
fn answered_here(row: &FieldRow) -> bool {
    row.frame.standing == Standing::NeedsYou && !row.asking()
}
