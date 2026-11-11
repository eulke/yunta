//! One run of a field: the group it belongs in, what holds it where it is, the
//! command that moves it on, and the line a field's row draws it as. The page lays
//! the rows out; a document that lists a run says it with the same words.

use std::cmp::Ordering;
use std::time::Duration;

use surco::components::{Edge, RunLine};
use surco::glyphs::Glyphs;
use surco::words::Standing;
use yunta_core::step::NextStep;
use yunta_replay::frame::{RunFrame, RunPhase};

use crate::{command, engine, pause};

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

/// One run of a field: the run as replay framed it, and whether it stopped
/// on a decision `yunta status` lists the options of. The page says it, and
/// so does any document that lists the run.
#[derive(Debug, Clone)]
pub struct FieldRow {
    pub frame: RunFrame,
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
            && self.frame.asking.is_some()
    }

    /// How long the run has been where it is — what the rows of a group are
    /// ordered by: the time since its last event, which is the event that
    /// put it there. Zero for a log with none.
    pub fn stood(&self) -> Duration {
        self.frame.since_last_event.unwrap_or_default()
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
                frame.asking.as_ref().map(engine::asking_in_full)
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

    /// The command that moves the run on, when a person has one to run:
    /// asked live, a question is answered where it is asked, and no command
    /// here reaches it.
    pub fn command(&self) -> Option<String> {
        let frame = &self.frame;
        let run = frame.run_id.clone();
        match (frame.standing, &frame.phase) {
            (Standing::Stalled, _) => Some(command::line(&NextStep::Resume { run })),
            (Standing::NeedsYou, RunPhase::Created | RunPhase::Running) => None,
            (_, RunPhase::Waiting { .. }) => Some(match self.menu {
                true => command::line(&NextStep::status(&run)),
                false => command::line(&NextStep::Resume { run }),
            }),
            (_, RunPhase::Broken { .. }) => Some(command::line(&NextStep::Verify { run })),
            _ => None,
        }
    }

    /// What holds the run, on the one line a field's row has room for: the
    /// node a live engine asks about, the claim a parked run stopped on, the
    /// stall, how far a moving run has come, what failed a closed one.
    /// Nothing for a run that finished.
    pub fn said(&self, glyphs: Glyphs) -> String {
        let frame = &self.frame;
        match (frame.standing, &frame.phase) {
            (Standing::Stalled, _) => engine::STALLED.to_string(),
            (Standing::NeedsYou, RunPhase::Created | RunPhase::Running) => frame
                .asking
                .as_ref()
                .map(engine::asking_on)
                .unwrap_or_default(),
            (_, RunPhase::Waiting { on }) => pause::parked_claim(on),
            (_, RunPhase::Broken { diagnostic }) => surco::text::one_line(diagnostic),
            (_, RunPhase::Created | RunPhase::Running) => crate::progress::row(frame, glyphs),
            (
                _,
                RunPhase::Failed {
                    failure: Some(failure),
                },
            ) => crate::failure::headline(failure),
            (
                _,
                RunPhase::Finished
                | RunPhase::Failed { failure: None }
                | RunPhase::Cancelled
                | RunPhase::Promoted { .. },
            ) => String::new(),
        }
    }

    /// The run as a field's row draws it, named `handle`, with `edge` at its
    /// far end and `glyphs` drawing what it says.
    pub fn line(&self, handle: String, edge: Edge, glyphs: Glyphs) -> RunLine {
        RunLine {
            standing: self.frame.standing,
            handle,
            workflow: self.frame.workflow.to_string(),
            mode: self.frame.mode.to_string(),
            furrow: self.frame.nodes.iter().map(|node| node.standing).collect(),
            said: self.said(glyphs),
            edge,
            pointed: false,
            marked: false,
        }
    }
}
