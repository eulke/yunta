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
//! Which runs it lists, and in which group, is the [`Inbox`]'s answer;
//! this module lays it out.

use surco::components::{Edge, Fields, Furrow, Gist, Headline, Next, Note, RunLine, Runs, Said};
use surco::doc::{Block, Depth, Doc};
use surco::figures::age;
use surco::glyphs::{Glyphs, Mark};
use surco::look::Look;
use surco::text::counted;
use surco::words::{Standing, NEED_YOU, NOTHING_NEEDS_YOU};
use yunta_core::step::{NextStep, Reach};
use yunta_render::command;
use yunta_render::page::Page;

use super::inbox::{Group, Inbox, RunRow};
use crate::context::Context;
use crate::error::CliError;
use crate::json::SCHEMA_VERSION;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};

/// How many unreadable runs a glance names before it counts the rest:
/// enough to recognise them by, few enough that the line stays a line.
const UNREADABLE_NAMED: usize = 3;

/// The runs of the repository this is run in — every run on the machine
/// with `all`, or when this is run outside a repository — as the field,
/// as deep as `depth` asks.
pub async fn list_runs(
    all: bool,
    depth: Depth,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let inbox = Inbox::gather(&ctx, all).await?;
    let answer = ListRunsAnswer {
        schema_version: SCHEMA_VERSION,
        storage: ctx.project.storage_path.display().to_string(),
        inbox,
        depth,
    };
    emit(output, &answer, look)
}

/// The listing as a document carries it: the runs it is about, each read
/// off its own frame, and where this project keeps its runs, which a
/// listing with none names.
#[derive(serde::Serialize)]
pub(crate) struct ListRunsAnswer {
    schema_version: u32,
    storage: String,
    #[serde(flatten)]
    inbox: Inbox,
    #[serde(skip)]
    depth: Depth,
}

/// The field as a person reads it: what needs them first, then every
/// other run by where it stands, as deep as `depth` asks, and where this
/// project keeps its runs when there are none.
struct Field<'a> {
    inbox: &'a Inbox,
    /// Where this project keeps its runs, which a field with none names.
    storage: &'a str,
    depth: Depth,
}

impl Page for Field<'_> {
    fn document(&self, look: &Look) -> Doc {
        let doc = Doc::new().with(self.headline()).with(Furrow(Vec::new()));
        if self.is_empty() {
            let gist = match self.inbox.elsewhere {
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
        doc.with_all(self.parts())
            .with_all(self.unreadable())
            .with_all(self.footer(look.glyphs))
            .with(self.next())
    }
}

impl Answer for ListRunsAnswer {
    fn page(&self) -> impl Page + '_ {
        Field {
            inbox: &self.inbox,
            storage: &self.storage,
            depth: self.depth,
        }
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

impl Field<'_> {
    /// Whether the field has no run to show at all.
    fn is_empty(&self) -> bool {
        self.inbox.rows.is_empty() && self.inbox.unreadable.is_empty()
    }

    /// How many runs stand in `group`.
    fn count(&self, group: Group) -> usize {
        self.inbox
            .rows
            .iter()
            .filter(|row| row.group == group)
            .count()
    }

    /// The field's headline: what needs a person — or that nothing does —
    /// and how many runs stand anywhere else.
    fn headline(&self) -> Headline {
        let (needs, stalled) = (self.count(Group::NeedsYou), self.count(Group::Stalled));
        let (mark, said) = match (needs, stalled) {
            (0, 0) if self.count(Group::Running) > 0 => {
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
        let subject = match self.inbox.here {
            true => "runs of this repository",
            false => "runs on this machine",
        };
        let mut headline = Headline::new(mark, subject, said);
        if needs > 0 && stalled > 0 {
            headline = headline.fact(format!("{stalled} {}", Standing::Stalled.word()));
        }
        for (group, word) in [
            (Group::Stopped, Standing::Stopped.word()),
            (Group::Running, Standing::Running.word()),
            (Group::Closed, "closed"),
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
    fn parts(&self) -> Vec<Block> {
        let mut rows: Vec<&RunRow> = self.inbox.rows.iter().collect();
        rows.sort_by(|a, b| b.age.cmp(&a.age).then_with(|| a.run_id.cmp(&b.run_id)));
        let of = |group: Group, asking: bool| -> Vec<&RunRow> {
            rows.iter()
                .copied()
                .filter(|row| row.group == group && (row.asking == asking))
                .collect()
        };
        let mut closed = of(Group::Closed, false);
        closed.reverse();
        let needs_heading = format!("{}, oldest first", Standing::NeedsYou.word());
        let mut parts = vec![
            ("asking now", of(Group::NeedsYou, true)),
            (needs_heading.as_str(), of(Group::NeedsYou, false)),
            (Standing::Stalled.word(), of(Group::Stalled, false)),
            (Standing::Stopped.word(), of(Group::Stopped, false)),
            (Standing::Running.word(), of(Group::Running, false)),
        ];
        if self.depth.reaches(Depth::Full) {
            parts.push(("closed, newest first", closed));
        }
        parts
            .into_iter()
            .filter(|(_, rows)| !rows.is_empty())
            .flat_map(|(heading, rows)| {
                let runs = Runs {
                    rows: rows.into_iter().map(|row| self.line(row)).collect(),
                };
                [Block::Heading(heading.to_string()), runs.into()]
            })
            .collect()
    }

    /// One run's row: its handle — its whole id for a reader who digs —
    /// and at its edge the command that moves a stalled or stopped run on,
    /// or how long any other has stood where it is.
    fn line(&self, row: &RunRow) -> RunLine {
        let handle = match self.depth.reaches(Depth::Dig) {
            true => row.run_id.to_string(),
            false => row.run_id.handle().to_string(),
        };
        let edge = match (row.group, &row.command) {
            (Group::Stalled | Group::Stopped, Some(command)) => Edge::Command(command.clone()),
            _ => Edge::Age(age(row.age)),
        };
        row.line(handle, edge)
    }

    /// The runs that do not read back, each with why, in the whole story.
    fn unreadable(&self) -> Vec<Block> {
        if !self.depth.reaches(Depth::Full) || self.inbox.unreadable.is_empty() {
            return Vec::new();
        }
        let mut unreadable: Vec<_> = self.inbox.unreadable.iter().collect();
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
            .inbox
            .rows
            .iter()
            .filter(|row| row.group == Group::Closed);
        if let (false, Some(newest)) = (full, closed.map(|row| row.age).min()) {
            said.push(format!("the newest closed {} ago", age(newest)));
        }
        if !full && !self.inbox.unreadable.is_empty() {
            said.push(format!("{} could not be read", self.unreadable_handles()));
        }
        if self.inbox.elsewhere > 0 {
            let elsewhere = counted(self.inbox.elsewhere, "run");
            said.push(format!("{elsewhere} in other projects"));
        }
        (!said.is_empty()).then(|| Note(said.join(&format!(" {} ", glyphs.sep()))).into())
    }

    /// The runs that do not read back, by handle: the first few, and how
    /// many more there are.
    fn unreadable_handles(&self) -> String {
        let mut handles: Vec<&str> = self
            .inbox
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
            .inbox
            .needing_a_person()
            .first()
            .map(|row| command::line(&NextStep::status(&row.run_id)));
        if let Some(status) = longest {
            steps.push((status, "opens the run that has waited longest"));
        }
        let left_out = self.count(Group::Closed) > 0 || !self.inbox.unreadable.is_empty();
        if left_out && !self.depth.reaches(Depth::Full) {
            steps.push((
                command::line(&NextStep::ListRuns {
                    reach: Reach::WithClosed,
                }),
                "lists the closed runs too",
            ));
        }
        if self.inbox.elsewhere > 0 {
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
