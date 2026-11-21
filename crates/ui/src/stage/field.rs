//! See [`super`]. The field: the runs of this repository, or of this machine, as
//! render's `Field` page lists them, a pointer on the one a key acts on and the runs a
//! person picked for an act that takes several.
//!
//! The stage keeps none of the runs: it is drawn from the field the state keeps of what
//! it follows, framed at the instant of each draw, so a run moves to its group, and the
//! tally and the attention row count it, as its news come, and its age moves with the
//! clock. A look that failed is said over the field as it was last read, until the next
//! news. Widening the reach follows the machine's field, which comes whole in place of
//! the field shown.
//!
//! The pointer moves along the runs in the order the page lists them, which the page
//! says ([`Field::listed`]), and is kept by the run it stands on, so it stays on a run
//! that moves; where its run left the field, the first run listed stands for it, and a
//! pick whose run left goes. Resuming acts at once; cancelling and closing ask a
//! confirmation first, closing naming every run it closes. No act asks for the field:
//! its runs move as their news come. Opening the run pointed at, or the one that has
//! waited longest on a person, opens its stage over the field; so do the workflows, with
//! `w`, and doctor, with `m`. A person who comes back to the field finds it as the state
//! keeps it, and nothing is asked; one who leaves a run for it finds noted there, until the
//! next key, whether the run keeps going, waits or ended.

use std::path::Path;

use chrono::{DateTime, Utc};
use surco::components::{Attention, Count, Note, Region, Screen, Sight, Tally};
use surco::doc::{Depth, Doc};
use surco::look::Look;
use surco::words::Standing;
use yunta_api::{FieldReach, FieldReading};
use yunta_core::step::{NextStep, Reach};
use yunta_core::RunId;
use yunta_render::page::{Field, FieldGroup, Page};

use super::{Act, Want};
use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::{FollowedRuns, Operation, Outcome};

/// The standings a screen's tally counts, in the order it says them.
const TALLIED: [Standing; 4] = [
    Standing::NeedsYou,
    Standing::Stalled,
    Standing::Stopped,
    Standing::Running,
];

/// The field a person moves through: where they stand in it, and how far and how deep
/// it reads. Not the field's runs, which the state keeps of what it follows.
#[derive(Debug, Clone)]
pub struct FieldStage {
    /// The run a key acts on, where a person moved onto one: where the field does not
    /// list it, its first run stands for it.
    pointed: Option<RunId>,
    /// The runs picked for an act that takes several, in the order they were picked.
    marked: Vec<RunId>,
    pub(super) reach: FieldReach,
    depth: Depth,
    /// What the last act came to where it did not do what it was asked, or what leaving a
    /// run said of it: noted until the next key.
    said: Option<String>,
}

/// The field the state keeps, framed at an instant, beside where the project keeps its
/// runs: what the stage draws and acts on.
struct Framed<'a> {
    reading: FieldReading,
    storage: &'a Path,
}

impl FieldStage {
    /// The field as far as `reach`, at a glance, the pointer on its first run.
    pub(crate) fn reaching(reach: FieldReach) -> Self {
        FieldStage {
            pointed: None,
            marked: Vec::new(),
            reach,
            depth: Depth::Glance,
            said: None,
        }
    }

    /// `said` noted on the field until the next key, where anything was said: what leaving a
    /// run for it said of the run.
    pub(crate) fn noting(&mut self, said: Option<String>) {
        self.said = said;
    }

    /// The field `followed` keeps, framed at `now`: none until it is read. A field read
    /// at the reach before stands until the field at this one comes in its place.
    fn framed<'a>(followed: &'a FollowedRuns, now: DateTime<Utc>) -> Option<Framed<'a>> {
        let field = followed.field.as_ref()?;
        Some(Framed {
            reading: field.reading(now),
            storage: followed.storage.as_deref()?,
        })
    }

    /// `framed` as render's page draws it, the pointer on `pointed`.
    fn page<'a>(&'a self, framed: &'a Framed<'_>, pointed: Option<&'a RunId>) -> Field<'a> {
        Field {
            rows: &framed.reading.rows,
            unreadable: &framed.reading.unreadable,
            elsewhere: framed.reading.elsewhere,
            here: framed.reading.here,
            storage: framed.storage,
            depth: self.depth,
            pointed,
            marked: &self.marked,
        }
    }

    /// The runs the page lists of `framed`, in its order: none where the field is not
    /// read.
    fn listed(&self, framed: Option<&Framed<'_>>) -> Vec<RunId> {
        framed
            .map(|framed| {
                self.page(framed, None)
                    .listed()
                    .into_iter()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The run a key acts on among `listed`: the one pointed at where it is listed, the
    /// first run listed otherwise.
    fn pointed_in(&self, listed: &[RunId]) -> Option<RunId> {
        let pointed = self.pointed.as_ref().filter(|run| listed.contains(run));
        pointed.or(listed.first()).cloned()
    }

    /// The pointer and the picks kept to `listed`: the picks whose runs it lists, and the
    /// pointer on its run where it lists it, on the first run otherwise.
    fn kept_to(&mut self, listed: &[RunId]) {
        self.marked.retain(|run| listed.contains(run));
        self.pointed = self.pointed_in(listed);
    }

    /// The field `followed` keeps as a screen at `now`: the page, how many runs stand at
    /// each word that asks for a look, the run that has waited longest on a person, and
    /// the keys; a look that failed and what the last act came to said under the page.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Screen {
        let framed = FieldStage::framed(followed, now);
        let mut doc = match &framed {
            Some(framed) => {
                let pointed = self.pointed_in(&self.listed(Some(framed)));
                self.page(framed, pointed.as_ref()).document(look)
            }
            None => Doc::new().with(Note(yunta_render::screen::READING_THE_RUNS.to_string())),
        };
        for said in [&followed.look_failed, &self.said].into_iter().flatten() {
            doc = doc.with(Note(said.clone()));
        }
        let mut screen = Screen::new(Region::new(doc).in_sight(Sight::following_pointer()));
        screen.trail = vec![yunta_render::screen::YUNTA.to_string()];
        if let Some(framed) = &framed {
            screen.tally = Tally::Counted(tally(&framed.reading));
            screen.attention = waiting_longest(&framed.reading, look);
        }
        screen.legend = yunta_render::screen::field_legend(
            self.reach == FieldReach::Machine,
            self.depth.reaches(Depth::Full),
        );
        screen
    }

    /// What `stroke` asks for in the field `followed` keeps at `now`, the pointer and the
    /// picks first kept to what it lists.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        self.said = None;
        let framed = FieldStage::framed(followed, now);
        let listed = self.listed(framed.as_ref());
        if framed.is_some() {
            self.kept_to(&listed);
        }
        match stroke {
            Stroke::Down | Stroke::Insert('j') => self.moved(1, &listed),
            Stroke::Up | Stroke::Insert('k') => self.moved(-1, &listed),
            Stroke::Enter => opened(self.pointed_in(&listed)),
            Stroke::Insert('n') => opened(
                framed
                    .as_ref()
                    .and_then(|framed| run_waiting_longest(&framed.reading)),
            ),
            Stroke::Insert(' ') => self.picked(&listed),
            Stroke::Insert('r') => self.act(&listed, |run| Want::Work(Operation::Resume { run })),
            Stroke::Insert('c') => self.act(&listed, |run| {
                let asks = yunta_render::screen::cancelling(run.handle());
                let same = vec![NextStep::Cancel { run: run.clone() }];
                Want::Confirm(Confirmation::new(asks, same, Operation::Cancel { run }))
            }),
            Stroke::Insert('x') => self.closing(&listed),
            Stroke::Insert('a') => {
                self.reach = match self.reach {
                    FieldReach::Here => FieldReach::Machine,
                    FieldReach::Machine => FieldReach::Here,
                };
                Vec::new()
            }
            Stroke::Insert('d') => {
                self.depth = match self.depth.reaches(Depth::Full) {
                    true => Depth::Glance,
                    false => Depth::Full,
                };
                Vec::new()
            }
            Stroke::Insert('w') => Want::taking(&NextStep::List),
            Stroke::Insert('m') => Want::taking(&NextStep::Doctor {
                sessions: false,
                full: false,
            }),
            Stroke::Insert('q') | Stroke::Decline | Stroke::Interrupt => vec![Want::Leave],
            _ => Vec::new(),
        }
    }

    /// What the field `followed` keeps at `now` offers, each with the key that takes it
    /// here: following the run pointed at, resuming and cancelling it, closing what `x`
    /// closes where one command does it, the run waiting on the person, the field as far
    /// again or with its closed runs, the workflows and doctor.
    pub(crate) fn offered(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        let framed = FieldStage::framed(followed, now);
        let listed = self.listed(framed.as_ref());
        let on_the_run = self.pointed_in(&listed).into_iter().flat_map(|run| {
            [
                (Stroke::Enter, NextStep::Follow { run: run.clone() }),
                (Stroke::Insert('r'), NextStep::Resume { run: run.clone() }),
                (Stroke::Insert('c'), NextStep::Cancel { run }),
            ]
        });
        let closing = self
            .closed_by_one_command(&listed)
            .map(|run| (Stroke::Insert('x'), NextStep::Close { run }));
        let needs = framed
            .as_ref()
            .and_then(|framed| run_waiting_longest(&framed.reading))
            .map(|_| (Stroke::Insert('n'), NextStep::Needs));
        let reach = match self.reach {
            FieldReach::Here => Reach::Machine,
            FieldReach::Machine => Reach::Repository,
        };
        let closed = match self.depth.reaches(Depth::Full) {
            true => Reach::Repository,
            false => Reach::WithClosed,
        };
        let always = [
            (Stroke::Insert('a'), NextStep::ListRuns { reach }),
            (Stroke::Insert('d'), NextStep::ListRuns { reach: closed }),
            (Stroke::Insert('w'), NextStep::List),
            (
                Stroke::Insert('m'),
                NextStep::Doctor {
                    sessions: false,
                    full: false,
                },
            ),
        ];
        on_the_run
            .chain(closing)
            .chain(needs)
            .chain(always)
            .map(|(key, step)| Act::by(key, step))
            .collect()
    }

    /// What an answer to work the field asked for does: the runs closed no longer
    /// picked, and what refused or failed said. The field moves as its runs do, so no
    /// answer asks for it.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Closed {
                closed,
                refused,
                failed,
            } => {
                self.marked.retain(|run| !closed.contains(run));
                self.said = failed.or_else(|| refused.first().map(ToString::to_string));
            }
            Outcome::Refused(refused) => self.said = Some(refused.to_string()),
            Outcome::Failed(said) => self.said = Some(said),
            Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::Started { .. }
            | Outcome::PullRequest(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Stopped => {}
        }
        Vec::new()
    }

    /// The pointer moved `by` rows along `listed`, around at either end.
    fn moved(&mut self, by: isize, listed: &[RunId]) -> Vec<Want> {
        if listed.is_empty() {
            return Vec::new();
        }
        let at = self
            .pointed_in(listed)
            .and_then(|run| listed.iter().position(|listed| *listed == run))
            .unwrap_or(0);
        let to = (at as isize + by).rem_euclid(listed.len() as isize) as usize;
        self.pointed = Some(listed[to].clone());
        Vec::new()
    }

    /// The run pointed at among `listed` picked, or put back.
    fn picked(&mut self, listed: &[RunId]) -> Vec<Want> {
        if let Some(run) = self.pointed_in(listed) {
            match self.marked.iter().position(|marked| *marked == run) {
                Some(at) => {
                    self.marked.remove(at);
                }
                None => self.marked.push(run),
            }
        }
        Vec::new()
    }

    /// `act` on the run pointed at among `listed`, where there is one.
    fn act(&self, listed: &[RunId], act: impl FnOnce(RunId) -> Want) -> Vec<Want> {
        self.pointed_in(listed).map(act).into_iter().collect()
    }

    /// The runs picked among `listed`, in the order they were picked.
    fn marked_in(&self, listed: &[RunId]) -> Vec<RunId> {
        let marked = self.marked.iter().filter(|run| listed.contains(run));
        marked.cloned().collect()
    }

    /// The one run `x` closes among `listed`, where it closes one: the run picked, or
    /// the one pointed at where none is. Several picked are closed together, which no
    /// one command does, so the palette offers that by its key alone.
    fn closed_by_one_command(&self, listed: &[RunId]) -> Option<RunId> {
        match self.marked_in(listed).as_slice() {
            [] => self.pointed_in(listed),
            [only] => Some(only.clone()),
            _ => None,
        }
    }

    /// Closing the runs picked among `listed`, or the one pointed at where none is,
    /// asked first with every run it closes named.
    fn closing(&self, listed: &[RunId]) -> Vec<Want> {
        let runs = match self.marked_in(listed) {
            marked if marked.is_empty() => self.pointed_in(listed).into_iter().collect(),
            marked => marked,
        };
        if runs.is_empty() {
            return Vec::new();
        }
        let handles: Vec<&str> = runs.iter().map(RunId::handle).collect();
        let asks = yunta_render::screen::closing(&handles);
        let same = runs
            .iter()
            .map(|run| NextStep::Close { run: run.clone() })
            .collect();
        vec![Want::Confirm(Confirmation::new(
            asks,
            same,
            Operation::Close { runs },
        ))]
    }
}

/// How many open runs of `reading` stand at each word a screen's tally counts, the words
/// none stands at left out.
fn tally(reading: &FieldReading) -> Vec<Count> {
    TALLIED
        .into_iter()
        .map(|standing| Count {
            standing,
            runs: reading
                .rows
                .iter()
                .filter(|row| row.group() != FieldGroup::Closed && row.frame.standing == standing)
                .count(),
        })
        .filter(|count| count.runs > 0)
        .collect()
}

/// The run of `reading` that has waited longest on a person, and the command that
/// answers it.
fn waiting_longest(reading: &FieldReading, look: &Look) -> Option<Attention> {
    let row = *reading.needing_a_person().first()?;
    let command = row.command()?;
    let mut attention = Attention::command(row.said(look.glyphs), command);
    attention.subject = Some(row.frame.run_id.handle().to_string());
    Some(attention)
}

/// The run of `reading` that has waited longest on a person, where one waits.
fn run_waiting_longest(reading: &FieldReading) -> Option<RunId> {
    let longest = reading.needing_a_person().first().copied()?;
    Some(longest.frame.run_id.clone())
}

/// `run` followed in its stage, opened over the field, where there is a run.
fn opened(run: Option<RunId>) -> Vec<Want> {
    run.map(|run| Want::taking(&NextStep::status(&run)))
        .unwrap_or_default()
}
