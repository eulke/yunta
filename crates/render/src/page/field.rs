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
//! framed it, which its row says, and this module lays it out.

use std::path::Path;

use surco::components::{Edge, Fields, Furrow, Gist, Headline, Next, Note, RunLine, Runs, Said};
use surco::doc::{Block, Depth, Doc};
use surco::figures::age;
use surco::glyphs::{Glyphs, Mark};
use surco::look::Look;
use surco::text::counted;
use surco::words::{Standing, NEED_YOU, NOTHING_NEEDS_YOU};
use yunta_core::step::{NextStep, Reach};
use yunta_core::RunId;

mod row;
mod unreadable;
pub use row::{FieldGroup, FieldRow};
pub use unreadable::{RunUnread, UnreadableRun};

use super::Page;
use crate::command;

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
    pub storage: &'a Path,
    pub depth: Depth,
    /// The run a key acts on, where a person moves through the field; none on a field
    /// that is only read.
    pub pointed: Option<&'a RunId>,
    /// The runs a person picked for an act that takes several.
    pub marked: &'a [RunId],
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
            let kept = Fields::new().push_if("kept in", self.storage.display().to_string());
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

    /// Every run the field lists, in the order it lists them: what a person moving
    /// through it moves along.
    pub fn listed(&self) -> Vec<&RunId> {
        self.groups()
            .into_iter()
            .flat_map(|(_, rows)| rows.into_iter().map(|row| &row.frame.run_id))
            .collect()
    }

    /// The runs, a group to each thing a reader can do about them, each
    /// under its heading, as [`Field::groups`] orders them.
    fn parts(&self, glyphs: Glyphs) -> Vec<Block> {
        self.groups()
            .into_iter()
            .flat_map(|(heading, rows)| {
                let runs = Runs {
                    rows: rows.into_iter().map(|row| self.line(row, glyphs)).collect(),
                };
                [Block::Heading(heading), runs.into()]
            })
            .collect()
    }

    /// The runs, a group to each thing a reader can do about them, each under its
    /// heading, the empty ones left out: the run that has waited longest first, and
    /// runs that came to where they stand together by id, which for a ULID is the
    /// order they were created in. The closed are the other way round, newest first,
    /// and only the whole story lists them.
    fn groups(&self) -> Vec<(String, Vec<&FieldRow>)> {
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
        let mut groups = vec![
            ("asking now".to_string(), of(FieldGroup::NeedsYou, true)),
            (
                format!("{}, oldest first", Standing::NeedsYou.word()),
                of(FieldGroup::NeedsYou, false),
            ),
            (
                Standing::Stalled.word().to_string(),
                of(FieldGroup::Stalled, false),
            ),
            (
                Standing::Stopped.word().to_string(),
                of(FieldGroup::Stopped, false),
            ),
            (
                Standing::Running.word().to_string(),
                of(FieldGroup::Running, false),
            ),
        ];
        if self.depth.reaches(Depth::Full) {
            groups.push(("closed, newest first".to_string(), closed));
        }
        groups.retain(|(_, rows)| !rows.is_empty());
        groups
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
        RunLine {
            pointed: self.pointed == Some(run_id),
            marked: self.marked.contains(run_id),
            ..row.line(handle, edge, glyphs)
        }
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
