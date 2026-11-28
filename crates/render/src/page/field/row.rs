//! One run of a field: the group it belongs in, what holds it where it is, the
//! command that moves it on, and the line a field's row draws it as. The page lays
//! the rows out; a document that lists a run says it with the same words.

use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Duration;

use surco::components::{Edge, RunLine};
use surco::glyphs::Glyphs;
use surco::words::Standing;
use yunta_core::step::NextStep;
use yunta_replay::frame::{RunFrame, RunPhase, RunTimes};

use crate::command::{self, Reader, SaidStep};
use crate::{engine, pause};

/// The part of a field a run belongs in — the field's own grouping,
/// derived from the run's word so a heading can never disagree with the
/// summary printed under it.
///
/// The order of the variants is the order the groups print in: what
/// stopped on a person comes before what is still moving, which comes
/// before what is already closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldGroup {
    /// Stopped until a person acts: a decision to answer, or a log that
    /// stopped making sense.
    NeedsYou,
    /// Its log says it is moving and the engine that drove it is gone:
    /// it waits on a person as surely as a decision does, but what it
    /// takes is a resume, not an answer.
    Stalled,
    /// A person stopped it, and nothing is owed: a resume continues it
    /// whenever they choose.
    Stopped,
    /// Moving on its own, parked until a limit resets, or created and not
    /// yet started: the runs the field calls `running`.
    Running,
    /// Closed, however it closed.
    Closed,
}

impl FieldGroup {
    /// What the field calls the runs of the group, over their rows and in a printed field's
    /// headline counts: the word every surface calls such a run by, and `closed` however it
    /// closed. Not the heading the runs that need a person stand under, which says their
    /// order too.
    pub fn word(self) -> &'static str {
        match self {
            FieldGroup::NeedsYou => Standing::NeedsYou.word(),
            FieldGroup::Stalled => Standing::Stalled.word(),
            FieldGroup::Stopped => Standing::Stopped.word(),
            FieldGroup::Running => Standing::Running.word(),
            FieldGroup::Closed => "closed",
        }
    }

    /// Which group a run called `standing` belongs in, `closed` saying
    /// whether its log closed it. A log that stopped making sense needs a
    /// person as much as a decision does: nothing moves it on its own
    /// again.
    ///
    /// The grouping is read off the word every other surface calls the
    /// run by, so a heading and the summary under it are two views of
    /// one answer rather than two readings of a phase.
    pub fn of(standing: Standing, closed: bool) -> Self {
        match standing {
            _ if closed => FieldGroup::Closed,
            Standing::NeedsYou | Standing::Broken => FieldGroup::NeedsYou,
            Standing::Stalled => FieldGroup::Stalled,
            Standing::Stopped => FieldGroup::Stopped,
            Standing::Created | Standing::Running | Standing::Waiting => FieldGroup::Running,
            Standing::Finished
            | Standing::Reported
            | Standing::Failed
            | Standing::Rerouted
            | Standing::Promoted
            | Standing::Skipped
            | Standing::NeverRan => FieldGroup::Closed,
        }
    }
}

/// One run of a field: the run as replay framed it at its head, what the
/// clock adds to it at the instant the field is read, and whether it stopped
/// on a decision `yunta status` lists the options of. The page says it, and
/// so does any document that lists the run.
#[derive(Debug, Clone)]
pub struct FieldRow {
    /// Shared with whoever keeps the run's frame: a row is made at every
    /// reading of the field, its frame only when the run moves.
    pub frame: Arc<RunFrame>,
    pub times: RunTimes,
    pub menu: bool,
}

impl FieldRow {
    /// The group of the field the run belongs in.
    pub fn group(&self) -> FieldGroup {
        let closed = matches!(
            self.frame.phase,
            RunPhase::Finished
                | RunPhase::Failed { .. }
                | RunPhase::Cancelled
                | RunPhase::Promoted { .. }
        );
        FieldGroup::of(self.frame.standing, closed)
    }

    /// Whether its engine is asking a person at a terminal now.
    pub fn asking(&self) -> bool {
        self.frame.standing == Standing::NeedsYou
            && matches!(self.frame.phase, RunPhase::Created | RunPhase::Running)
            && self.times.asking.is_some()
    }

    /// How long the run has been where it is — what the rows of a group are
    /// ordered by: the time since its last event, which is the event that
    /// put it there. Zero for a log with none.
    pub fn stood(&self) -> Duration {
        self.times.since_last_event.unwrap_or_default()
    }

    /// Which of `a` and `b` a group lists first: the one that has waited
    /// longest, and of two that arrived together, the lower id.
    pub fn by_wait(a: &FieldRow, b: &FieldRow) -> Ordering {
        b.stood()
            .cmp(&a.stood())
            .then_with(|| a.frame.run_id.cmp(&b.frame.run_id))
    }

    /// What holds the run where it is, said whole, with `glyphs` drawing how
    /// far a moving run has come; `None` for a run nothing holds.
    pub fn reason(&self, glyphs: Glyphs) -> Option<String> {
        let frame = &self.frame;
        match (frame.standing, &frame.phase) {
            (Standing::Stalled, _) => Some(engine::STALLED.to_string()),
            (Standing::NeedsYou, RunPhase::Created | RunPhase::Running) => {
                self.times.asking.as_ref().map(engine::asking_in_full)
            }
            (_, RunPhase::Waiting { on }) => Some(pause::parked_in_full(on)),
            (_, RunPhase::Broken { diagnostic }) => Some(surco::text::one_line(diagnostic)),
            (_, RunPhase::Created | RunPhase::Running) => {
                Some(crate::progress::line(frame, glyphs))
            }
            (
                _,
                RunPhase::Failed {
                    failure: Some(failure),
                },
            ) => Some(crate::failure::headline(failure)),
            (
                _,
                RunPhase::Finished
                | RunPhase::Failed { failure: None }
                | RunPhase::Cancelled
                | RunPhase::Promoted { .. },
            ) => None,
        }
    }

    /// The step that moves the run on, when a person has one to take: asked
    /// live, a question is answered where it is asked, and no step here
    /// reaches it.
    pub fn step(&self) -> Option<NextStep> {
        let frame = &self.frame;
        let run = frame.run_id.clone();
        match (frame.standing, &frame.phase) {
            (Standing::Stalled, _) => Some(NextStep::Resume { run }),
            (Standing::NeedsYou, RunPhase::Created | RunPhase::Running) => None,
            (_, RunPhase::Waiting { .. }) => Some(match self.menu {
                true => NextStep::status(&run),
                false => NextStep::Resume { run },
            }),
            (_, RunPhase::Broken { .. }) => Some(NextStep::Verify { run }),
            _ => None,
        }
    }

    /// The command that moves the run on, as [`FieldRow::step`] takes it.
    pub fn command(&self) -> Option<String> {
        self.step().map(|step| command::line(&step))
    }

    /// The command that moves the run on, beside what it does, as a page's next steps say it:
    /// as the act the run offers that takes it, as `yunta status` says that act (from its log
    /// for a stalled run, where it stopped for a stopped one), and as the step alone where the
    /// run offers no act that takes it.
    pub fn moved_on(&self) -> Option<SaidStep> {
        let step = self.step()?;
        let offered = self.frame.actions(None);
        Some(
            match offered.into_iter().find(|action| action.step == step) {
                Some(action) => SaidStep::of_action(&action, Reader::Person),
                None => SaidStep::of(&step),
            },
        )
    }

    /// What holds the run, on the one line a field's row has room for: the
    /// node a live engine asks about, what holds a parked run as its glance and
    /// its page say it ([`pause::holding`]), the stall, how far a moving run has come; a closed run's gist, as its page
    /// says it: the node that failed and how, the findings still blocking one
    /// that finished. Nothing for a run that closed with nothing to say.
    pub fn said(&self) -> String {
        let frame = &self.frame;
        match (frame.standing, &frame.phase) {
            (Standing::Stalled, _) => engine::STALLED.to_string(),
            (Standing::NeedsYou, RunPhase::Created | RunPhase::Running) => self
                .times
                .asking
                .as_ref()
                .map(engine::asking_on)
                .unwrap_or_default(),
            (_, RunPhase::Waiting { on }) => pause::holding(on),
            (_, RunPhase::Broken { diagnostic }) => surco::text::one_line(diagnostic),
            (_, RunPhase::Created | RunPhase::Running) => crate::progress::row(frame),
            (
                _,
                RunPhase::Finished
                | RunPhase::Failed { .. }
                | RunPhase::Cancelled
                | RunPhase::Promoted { .. },
            ) => crate::state::gist(frame, &self.times).unwrap_or_default(),
        }
    }

    /// The run as a field's row draws it, named `handle`, with `edge` at its
    /// far end.
    pub fn line(&self, handle: String, edge: Edge) -> RunLine {
        RunLine {
            standing: self.frame.standing,
            handle,
            workflow: self.frame.workflow.to_string(),
            mode: self.frame.mode.to_string(),
            furrow: self.frame.nodes.iter().map(|node| node.standing).collect(),
            said: self.said(),
            edge,
            pointed: false,
            marked: false,
        }
    }
}
