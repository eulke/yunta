//! The field — this repository's runs, or every run on the machine — as the page `yunta list
//! --runs` and `yunta` alone off a terminal print ([`Field`]), and as a screen lists it
//! ([`FieldRows`]).
//!
//! Both list the runs through one [`FieldRows::groups`]: a group to each thing a reader can
//! do about them — a run asking now, the runs waiting on a decision, the stalled, the stopped,
//! the moving — each under its word, the run that has waited longest first; what has closed,
//! newest first, in the whole story. The page adds what a printed listing says around them:
//! its headline, which counts what needs a person and what else is there; what a glance leaves
//! out; the runs that do not read back, whole, in the whole story; and what to type next. The
//! screen's list opens on the row that asks what to build, and ends with the runs that do not
//! read back at every depth, each a row with why, where a pointer can stand: the tally, the
//! attention row, the legend and the palette say the rest around it.
//!
//! Which runs it lists is its caller's to read: each row is a run as replay
//! framed it, which its row says, and this module lays it out.

use std::path::Path;

use surco::components::{Edge, Fields, Furrow, Gist, Headline, Next, Note, RunLine, Runs, Said};
use surco::doc::{Block, Depth, Doc};
use surco::figures::age;
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use surco::words::{Standing, NEED_YOU, NOTHING_NEEDS_YOU};
use yunta_core::step::{NextStep, Reach};
use yunta_core::RunId;

mod glance;
mod row;
mod unreadable;
pub use glance::{GlanceAct, RunAtAGlance};
pub use row::{FieldGroup, FieldRow};
pub use unreadable::{RunUnread, UnreadableRun};

use super::{FirstRequest, Page};
use crate::command::{self, SaidStep};

/// How many unreadable runs a glance names before it counts the rest:
/// enough to recognise them by, few enough that the line stays a line.
const UNREADABLE_NAMED: usize = 3;

/// What the field calls the runs a live engine asks about at their terminal, over their rows.
const ASKING_NOW: &str = "asking now";

/// What the field calls the runs it can name but not derive, over them.
const UNREADABLE: &str = "unreadable";

/// The field as a person reads it printed: what needs them first, then every
/// other run by where it stands, as deep as `depth` asks, and where this
/// project keeps its runs when there are none. Not [`FieldRows`], the field as a screen
/// lists it.
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
        doc.with_all(self.parts())
            .with_all(self.unreadable())
            .with_all(self.footer(look.glyphs))
            .with(self.next())
    }
}

impl<'a> Field<'a> {
    /// The runs as the field's list holds them, the page's groups drawn from it: nothing
    /// pointed at nor picked, and no run asked for, on a page that is only read.
    fn listing(&self) -> FieldRows<'a> {
        FieldRows {
            rows: self.rows,
            unreadable: self.unreadable,
            depth: self.depth,
            pointed: None,
            marked: &[],
            asks_for_a_run: false,
        }
    }

    /// How far the listing reaches: this repository's runs, or the runs on this machine.
    fn reach(&self) -> Reach {
        match self.here {
            true => Reach::Repository,
            false => Reach::Machine,
        }
    }

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
        for group in [FieldGroup::Stopped, FieldGroup::Running, FieldGroup::Closed] {
            let count = self.count(group);
            if count > 0 {
                headline = headline.fact(format!("{count} {}", group.word()));
            }
        }
        headline
    }

    /// The runs, a group to each thing a reader can do about them, each
    /// under its word, as [`FieldRows::groups`] orders them.
    fn parts(&self) -> Vec<Block> {
        self.listing().parts()
    }

    /// The steps that move each stalled or stopped run on, beside what each does to its run,
    /// in the order the field lists the runs: a row ends in its age, so the page names them
    /// among what a person types next.
    fn moved_on(&self) -> Vec<SaidStep> {
        let mut rows: Vec<&FieldRow> = self
            .rows
            .iter()
            .filter(|row| matches!(row.group(), FieldGroup::Stalled | FieldGroup::Stopped))
            .collect();
        rows.sort_by(|a, b| {
            a.group()
                .cmp(&b.group())
                .then_with(|| FieldRow::by_wait(a, b))
        });
        rows.into_iter().filter_map(FieldRow::moved_on).collect()
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
        vec![Block::Heading(UNREADABLE.to_string()), Said(said).into()]
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
        (!said.is_empty()).then(|| Note(said.join(&glyphs.spaced_sep())).into())
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
    /// the command that moves each stalled or stopped run on, the same reach
    /// with its closed runs, the runs of other projects — or, in a field with no run, the
    /// workflows to start one from; each act in the palette's words where the
    /// palette offers it.
    fn next(&self) -> Next {
        let mut steps = Vec::new();
        if self.is_empty() {
            steps.push(SaidStep::of(&NextStep::List));
        }
        let longest = self
            .rows
            .iter()
            .filter(|row| row.group() == FieldGroup::NeedsYou)
            .min_by(|a, b| FieldRow::by_wait(a, b))
            .map(|row| SaidStep::of(&NextStep::status(&row.frame.run_id)));
        steps.extend(longest);
        steps.extend(self.moved_on());
        let left_out = self.count(FieldGroup::Closed) > 0 || !self.unreadable.is_empty();
        if left_out && !self.depth.reaches(Depth::Full) {
            steps.push(SaidStep::of(&NextStep::ListRuns {
                reach: self.reach().with_closed(),
            }));
        }
        if self.elsewhere > 0 {
            steps.push(SaidStep::of(&NextStep::ListRuns {
                reach: Reach::Machine,
            }));
        }
        command::next(steps)
    }
}

/// The field as a screen lists it: the row that asks what to build, with the key that
/// starts a new run, where a run can be asked for; then its runs, a group to each thing a
/// person can do about them, as deep as `depth` asks; then the runs that do not read back,
/// at every depth, each with why.
/// The pointer stands on the run a key acts on, the picked mark on the runs picked for an act
/// that takes several. No headline and no next steps: the screen's tally, attention row,
/// legend and palette say them. Not [`Field`], the page `yunta list --runs` prints.
pub struct FieldRows<'a> {
    pub rows: &'a [FieldRow],
    /// The runs it can name but not derive.
    pub unreadable: &'a [UnreadableRun],
    pub depth: Depth,
    /// The run a key acts on, where a person moves through the field.
    pub pointed: Option<&'a RunId>,
    /// The runs a person picked for an act that takes several.
    pub marked: &'a [RunId],
    /// Whether a run can be asked for where the field stands, which its first row asks:
    /// outside a repository, and in one never set up, none can.
    pub asks_for_a_run: bool,
}

impl Page for FieldRows<'_> {
    fn document(&self, look: &Look) -> Doc {
        let first = self
            .asks_for_a_run
            .then(|| Block::Rows(vec![first_row(look.glyphs)]));
        Doc::new()
            .with_all(first)
            .with_all(self.parts())
            .with_all(self.unread())
    }
}

impl<'a> FieldRows<'a> {
    /// Every run the list holds, in the order it lists them, the runs that do not read back
    /// last: what a person moving through it moves along.
    pub fn listed(&self) -> Vec<&'a RunId> {
        let read = self.groups().into_iter().flat_map(|(_, rows)| rows);
        read.map(|row| &row.frame.run_id)
            .chain(self.unreadable_by_id().into_iter().map(|run| &run.run_id))
            .collect()
    }

    /// The runs, a group to each thing a reader can do about them, each under its word, the
    /// empty ones left out: the run that has waited longest first, and runs that came to
    /// where they stand together by id, which for a ULID is the order they were created in.
    /// The closed are the other way round, newest first, and only the whole story lists them.
    /// What the screen's list and the printed page both list.
    pub fn groups(&self) -> Vec<(String, Vec<&'a FieldRow>)> {
        let mut rows: Vec<&'a FieldRow> = self.rows.iter().collect();
        rows.sort_by(|a, b| FieldRow::by_wait(a, b));
        let of = |group: FieldGroup, asking: bool| -> Vec<&'a FieldRow> {
            rows.iter()
                .copied()
                .filter(|row| row.group() == group && (row.asking() == asking))
                .collect()
        };
        let mut closed = of(FieldGroup::Closed, false);
        closed.reverse();
        let mut groups: Vec<(String, Vec<&'a FieldRow>)> = vec![
            (
                heading(FieldGroup::NeedsYou, true),
                of(FieldGroup::NeedsYou, true),
            ),
            (
                heading(FieldGroup::NeedsYou, false),
                of(FieldGroup::NeedsYou, false),
            ),
        ];
        groups.extend(
            [
                FieldGroup::Stalled,
                FieldGroup::Stopped,
                FieldGroup::Running,
            ]
            .map(|group| (heading(group, false), of(group, false))),
        );
        if self.depth.reaches(Depth::Full) {
            groups.push((heading(FieldGroup::Closed, false), closed));
        }
        groups.retain(|(_, rows)| !rows.is_empty());
        groups
    }

    /// The runs, each group under its word, as [`FieldRows::groups`] orders them.
    fn parts(&self) -> Vec<Block> {
        self.groups()
            .into_iter()
            .flat_map(|(heading, rows)| {
                let runs = Runs {
                    rows: rows.into_iter().map(|row| self.line(row)).collect(),
                };
                [Block::Heading(heading), runs.into()]
            })
            .collect()
    }

    /// What a run is called on its row: its handle, or its whole id for a reader who digs.
    fn handle(&self, run_id: &RunId) -> String {
        match self.depth.reaches(Depth::Dig) {
            true => run_id.to_string(),
            false => run_id.handle().to_string(),
        }
    }

    /// One run's row: its handle and at its edge how long it has stood where it is, a
    /// stalled or stopped run's too: the command that moves one on is the page's next step,
    /// and the glance's.
    fn line(&self, row: &FieldRow) -> RunLine {
        let run_id = &row.frame.run_id;
        RunLine {
            pointed: self.pointed == Some(run_id),
            marked: self.marked.contains(run_id),
            ..row.line(self.handle(run_id), Edge::Age(age(row.stood())))
        }
    }

    /// The runs that do not read back, by id.
    fn unreadable_by_id(&self) -> Vec<&'a UnreadableRun> {
        let mut unreadable: Vec<&'a UnreadableRun> = self.unreadable.iter().collect();
        unreadable.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        unreadable
    }

    /// The runs that do not read back, under their word, a row each with why.
    fn unread(&self) -> Vec<Block> {
        let unreadable = self.unreadable_by_id();
        if unreadable.is_empty() {
            return Vec::new();
        }
        let rows = unreadable.into_iter().map(|run| RunLine {
            pointed: self.pointed == Some(&run.run_id),
            marked: self.marked.contains(&run.run_id),
            ..run.line(self.handle(&run.run_id))
        });
        let runs = Runs {
            rows: rows.collect(),
        };
        vec![Block::Heading(UNREADABLE.to_string()), runs.into()]
    }
}

/// What the field's list says over the runs of `group`, the runs a live engine asks about at
/// their terminal apart (`asking`): the group's word, and over the runs that need a person the
/// order they stand in.
fn heading(group: FieldGroup, asking: bool) -> String {
    match (group, asking) {
        (FieldGroup::NeedsYou, true) => ASKING_NOW.to_string(),
        (FieldGroup::NeedsYou, false) => format!("{}, oldest first", group.word()),
        (group, _) => group.word().to_string(),
    }
}

/// The field's first row, drawn in `glyphs`: what the first request asks, as a row of the list
/// says it, then the key that starts a new run.
fn first_row(glyphs: Glyphs) -> Line {
    Line::new()
        .push(Tone::Muted, glyphs.within())
        .plain(" ")
        .push(Tone::Muted, as_a_row(FirstRequest::QUESTION))
        .push(Tone::Faint, format!("  {} ", glyphs.sep()))
        .push(Tone::Strong, "n")
}

/// `question` as a row of the field's list says it: from a lowercase letter, as each of its
/// rows reads.
fn as_a_row(question: &str) -> String {
    let mut letters = question.chars();
    match letters.next() {
        Some(first) => first.to_lowercase().chain(letters).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests;
