//! `yunta list --runs`, and `yunta` alone in a repository: the field —
//! this repository's runs, or every run on the machine — as one page.
//!
//! Its headline counts what needs a person and what else is there. Its
//! rows come a group to each thing a reader can do about them — a run
//! asking now, the runs waiting on a decision, the stalled, the stopped,
//! the moving — and each group answers the run that has waited longest
//! first. What has closed is counted at a glance and listed, newest first,
//! in the whole story.
//!
//! Which runs it lists is its caller's to read: each row is a run as replay
//! framed it, and this module says it and lays it out.

use std::cmp::Ordering;
use std::time::Duration;

use surco::components::{Edge, Fields, Furrow, Gist, Headline, Next, Note, RunLine, Runs, Said};
use surco::doc::{Block, Depth, Doc};
use surco::figures::age;
use surco::glyphs::{Glyphs, Mark};
use surco::look::Look;
use surco::text::counted;
use surco::words::{Standing, NEED_YOU, NOTHING_NEEDS_YOU};
use yunta_core::step::{NextStep, Reach};
use yunta_core::RunId;
use yunta_replay::frame::{RunFrame, RunPhase};

use super::Page;
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
        }
    }
}

/// A run a field can name but not derive: its log or the manifest it froze
/// does not read back. It is listed as itself, never dropped — a run missing
/// from a listing is a run nobody goes looking for.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UnreadableRun {
    pub run_id: RunId,
    /// What stops it from reading back, as its reader said it.
    pub problem: String,
}

impl UnreadableRun {
    pub fn new(run_id: &RunId, problem: String) -> Self {
        UnreadableRun {
            run_id: run_id.clone(),
            problem,
        }
    }
}

/// How many unreadable runs a glance names before it counts the rest:
/// enough to recognise them by, few enough that the line stays a line.
const UNREADABLE_NAMED: usize = 3;

/// The field as a person reads it: what needs them first, then every
/// other run by where it stands, as deep as `depth` asks, and where this
/// project keeps its runs when there are none.
pub struct Field<'a> {
    pub rows: &'a [FieldRow],
    /// The runs it can name but not derive.
    pub unreadable: &'a [UnreadableRun],
    /// How many runs the listing leaves to other projects.
    pub elsewhere: usize,
    /// Whether the listing is the runs of the repository it was asked in,
    /// rather than every run on the machine.
    pub here: bool,
    /// Where this project keeps its runs, which a field with none names.
    pub storage: &'a str,
    pub depth: Depth,
}

impl Page for Field<'_> {
    fn document(&self, look: &Look) -> Doc {
        let doc = Doc::new().with(self.headline()).with(Furrow(Vec::new()));
        if self.is_empty() {
            let gist = match self.elsewhere {
                0 => "no runs yet",
                _ => "no runs in this repository yet",
            };
            // Where the runs would be: a reader who expected some checks it.
            let kept = Fields::new().push_if("kept in", self.storage);
            return doc
                .with(Gist::new(gist))
                .with(kept)
                .with_all(self.footer(look.glyphs))
                .with(self.next());
        }
        doc.with_all(self.parts(look.glyphs))
            .with_all(self.unreadable())
            .with_all(self.footer(look.glyphs))
            .with(self.next())
    }
}

impl Field<'_> {
    /// Whether the field has no run to show at all.
    fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.unreadable.is_empty()
    }

    /// How many runs stand in `group`.
    fn count(&self, group: FieldGroup) -> usize {
        self.rows.iter().filter(|row| row.group() == group).count()
    }

    /// The field's headline: what needs a person — or that nothing does —
    /// and how many runs stand anywhere else.
    fn headline(&self) -> Headline {
        let (needs, stalled) = (
            self.count(FieldGroup::NeedsYou),
            self.count(FieldGroup::Stalled),
        );
        let (mark, said) = match (needs, stalled) {
            (0, 0) if self.count(FieldGroup::Running) > 0 => {
                (Mark::Running, NOTHING_NEEDS_YOU.to_string())
            }
            (0, 0) => (Mark::Done, NOTHING_NEEDS_YOU.to_string()),
            (0, stalled) => (
                Mark::Caution,
                format!("{stalled} {}", Standing::Stalled.word()),
            ),
            (1, _) => (Mark::NeedsYou, format!("1 {}", Standing::NeedsYou.word())),
            (needs, _) => (Mark::NeedsYou, format!("{needs} {NEED_YOU}")),
        };
        let subject = match self.here {
            true => "runs of this repository",
            false => "runs on this machine",
        };
        let mut headline = Headline::new(mark, subject, said);
        if needs > 0 && stalled > 0 {
            headline = headline.fact(format!("{stalled} {}", Standing::Stalled.word()));
        }
        for (group, word) in [
            (FieldGroup::Stopped, Standing::Stopped.word()),
            (FieldGroup::Running, Standing::Running.word()),
            (FieldGroup::Closed, "closed"),
        ] {
            let count = self.count(group);
            if count > 0 {
                headline = headline.fact(format!("{count} {word}"));
            }
        }
        headline
    }

    /// The runs, a group to each thing a reader can do about them, each
    /// under its heading: the run that has waited longest first, and runs
    /// that came to where they stand together by id, which for a ULID is
    /// the order they were created in. The closed are the other way round,
    /// newest first, and only the whole story lists them.
    fn parts(&self, glyphs: Glyphs) -> Vec<Block> {
        let mut rows: Vec<&FieldRow> = self.rows.iter().collect();
        rows.sort_by(|a, b| FieldRow::by_wait(a, b));
        let of = |group: FieldGroup, asking: bool| -> Vec<&FieldRow> {
            rows.iter()
                .copied()
                .filter(|row| row.group() == group && (row.asking() == asking))
                .collect()
        };
        let mut closed = of(FieldGroup::Closed, false);
        closed.reverse();
        let needs_heading = format!("{}, oldest first", Standing::NeedsYou.word());
        let mut parts = vec![
            ("asking now", of(FieldGroup::NeedsYou, true)),
            (needs_heading.as_str(), of(FieldGroup::NeedsYou, false)),
            (Standing::Stalled.word(), of(FieldGroup::Stalled, false)),
            (Standing::Stopped.word(), of(FieldGroup::Stopped, false)),
            (Standing::Running.word(), of(FieldGroup::Running, false)),
        ];
        if self.depth.reaches(Depth::Full) {
            parts.push(("closed, newest first", closed));
        }
        parts
            .into_iter()
            .filter(|(_, rows)| !rows.is_empty())
            .flat_map(|(heading, rows)| {
                let runs = Runs {
                    rows: rows.into_iter().map(|row| self.line(row, glyphs)).collect(),
                };
                [Block::Heading(heading.to_string()), runs.into()]
            })
            .collect()
    }

    /// One run's row: its handle — its whole id for a reader who digs —
    /// and at its edge the command that moves a stalled or stopped run on,
    /// or how long any other has stood where it is.
    fn line(&self, row: &FieldRow, glyphs: Glyphs) -> RunLine {
        let run_id = &row.frame.run_id;
        let handle = match self.depth.reaches(Depth::Dig) {
            true => run_id.to_string(),
            false => run_id.handle().to_string(),
        };
        let edge = match (row.group(), row.command()) {
            (FieldGroup::Stalled | FieldGroup::Stopped, Some(command)) => Edge::Command(command),
            _ => Edge::Age(age(row.stood())),
        };
        row.line(handle, edge, glyphs)
    }

    /// The runs that do not read back, each with why, in the whole story.
    fn unreadable(&self) -> Vec<Block> {
        if !self.depth.reaches(Depth::Full) || self.unreadable.is_empty() {
            return Vec::new();
        }
        let mut unreadable: Vec<_> = self.unreadable.iter().collect();
        unreadable.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        let said = unreadable
            .iter()
            .map(|run| format!("{}: {}", run.run_id.handle(), run.problem))
            .collect();
        vec![
            Block::Heading("could not be read".to_string()),
            Said(said).into(),
        ]
    }

    /// What a glance leaves out, in one line: how recent the newest closed
    /// run is (the headline counts the closed runs), the runs that do not
    /// read back, and the runs this listing leaves to other projects.
    fn footer(&self, glyphs: Glyphs) -> Option<Block> {
        let full = self.depth.reaches(Depth::Full);
        let mut said = Vec::new();
        let closed = self
            .rows
            .iter()
            .filter(|row| row.group() == FieldGroup::Closed);
        if let (false, Some(newest)) = (full, closed.map(FieldRow::stood).min()) {
            said.push(format!("the newest closed {} ago", age(newest)));
        }
        if !full && !self.unreadable.is_empty() {
            said.push(format!("{} could not be read", self.unreadable_handles()));
        }
        if self.elsewhere > 0 {
            let elsewhere = counted(self.elsewhere, "run");
            said.push(format!("{elsewhere} in other projects"));
        }
        (!said.is_empty()).then(|| Note(said.join(&format!(" {} ", glyphs.sep()))).into())
    }

    /// The runs that do not read back, by handle: the first few, and how
    /// many more there are.
    fn unreadable_handles(&self) -> String {
        let mut handles: Vec<&str> = self
            .unreadable
            .iter()
            .map(|run| run.run_id.handle())
            .collect();
        handles.sort_unstable();
        let mut said = handles
            .iter()
            .take(UNREADABLE_NAMED)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        if let Some(more) = handles
            .len()
            .checked_sub(UNREADABLE_NAMED)
            .filter(|more| *more > 0)
        {
            said.push_str(&format!(" and {more} more"));
        }
        said
    }

    /// What a person types next: the run that has waited on them longest,
    /// the whole field, the runs of other projects — or, in a field with
    /// no run, the workflows to start one from.
    fn next(&self) -> Next {
        let mut steps = Vec::new();
        if self.is_empty() {
            steps.push((command::line(&NextStep::List), "the workflows you can run"));
        }
        let longest = self
            .rows
            .iter()
            .filter(|row| row.group() == FieldGroup::NeedsYou)
            .min_by(|a, b| FieldRow::by_wait(a, b))
            .map(|row| command::line(&NextStep::status(&row.frame.run_id)));
        if let Some(status) = longest {
            steps.push((status, "opens the run that has waited longest"));
        }
        let left_out = self.count(FieldGroup::Closed) > 0 || !self.unreadable.is_empty();
        if left_out && !self.depth.reaches(Depth::Full) {
            steps.push((
                command::line(&NextStep::ListRuns {
                    reach: Reach::WithClosed,
                }),
                "lists the closed runs too",
            ));
        }
        if self.elsewhere > 0 {
            steps.push((
                command::line(&NextStep::ListRuns {
                    reach: Reach::Machine,
                }),
                "every run on this machine",
            ));
        }
        Next { steps }
    }
}

#[cfg(test)]
mod tests;
