//! See [`super`]. The field: the runs of this repository, or of this machine, as
//! render's `Field` page lists them, a pointer on the one a key acts on and the runs a
//! person picked for an act that takes several.
//!
//! The pointer moves along the runs in the order the page lists them, which the page
//! says ([`Field::listed`]), and is kept by the run it stands on, so a field read again
//! keeps it where the run still is. Resuming acts at once; cancelling and closing ask a
//! confirmation first, closing naming every run it closes. Each act reads the field
//! again once it is done. Opening the run pointed at, or the one that has waited
//! longest on a person, opens its stage over the field, which is read again when the
//! person comes back to it; so do the workflows, with `w`, and doctor, with `m`.

use std::path::PathBuf;

use surco::components::{Attention, Count, Note, Region, Screen, Sight};
use surco::doc::Depth;
use surco::look::Look;
use surco::words::Standing;
use yunta_api::{FieldReach, FieldReading};
use yunta_core::step::{NextStep, Reach};
use yunta_core::RunId;
use yunta_render::page::{Field, FieldGroup, Page};

use super::{Act, CatalogStage, DoctorStage, RunStage, Stage, Want};
use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::{Operation, Outcome};

/// The standings a screen's tally counts, in the order it says them.
const TALLIED: [Standing; 4] = [
    Standing::NeedsYou,
    Standing::Stalled,
    Standing::Stopped,
    Standing::Running,
];

/// The field a person moves through.
#[derive(Debug, Clone)]
pub struct FieldStage {
    reading: FieldReading,
    /// Where the project keeps its runs, which a field with none names.
    storage: PathBuf,
    /// The run a key acts on.
    pointed: Option<RunId>,
    /// The runs picked for an act that takes several, in the order they were picked.
    marked: Vec<RunId>,
    reach: FieldReach,
    depth: Depth,
    /// What the last act came to where it did not do what it was asked.
    said: Option<String>,
}

impl FieldStage {
    /// The field `reading` lists, read as far as `reach`, the pointer on its first run.
    pub(crate) fn read(reading: FieldReading, storage: PathBuf, reach: FieldReach) -> Self {
        let mut stage = FieldStage {
            reading,
            storage,
            pointed: None,
            marked: Vec::new(),
            reach,
            depth: Depth::Glance,
            said: None,
        };
        stage.kept_to_listed();
        stage
    }

    /// The field read again, the pointer and the picks kept to what it lists.
    fn reread(&mut self, reading: FieldReading, storage: PathBuf) {
        self.reading = reading;
        self.storage = storage;
        self.kept_to_listed();
    }

    /// The pointer and the picks kept to the runs the page lists: the picks whose runs
    /// it still lists, and the pointer on its run where the page lists it, on the first
    /// run otherwise.
    fn kept_to_listed(&mut self) {
        let listed = self.listed();
        self.marked.retain(|run| listed.contains(run));
        if !self
            .pointed
            .as_ref()
            .is_some_and(|run| listed.contains(run))
        {
            self.pointed = listed.first().cloned();
        }
    }

    /// The field as render's page draws it.
    fn page(&self) -> Field<'_> {
        Field {
            rows: &self.reading.rows,
            unreadable: &self.reading.unreadable,
            elsewhere: self.reading.elsewhere,
            here: self.reading.here,
            storage: &self.storage,
            depth: self.depth,
            pointed: self.pointed.as_ref(),
            marked: &self.marked,
        }
    }

    /// The runs the page lists, in its order.
    fn listed(&self) -> Vec<RunId> {
        self.page().listed().into_iter().cloned().collect()
    }

    /// The field as a screen: the page, how many runs stand at each word that asks for
    /// a look, the run that has waited longest on a person, and the keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut doc = self.page().document(look);
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut screen = Screen::new(Region::new(doc).in_sight(Sight::following_pointer()));
        screen.trail = vec![yunta_render::screen::YUNTA.to_string()];
        screen.tally = TALLIED
            .into_iter()
            .map(|standing| Count {
                standing,
                runs: self.standing_at(standing),
            })
            .filter(|count| count.runs > 0)
            .collect();
        screen.attention = self.waiting_longest(look);
        screen.legend = yunta_render::screen::field_legend(
            self.reach == FieldReach::Machine,
            self.depth.reaches(Depth::Full),
        );
        screen
    }

    /// How many open runs stand at `standing`.
    fn standing_at(&self, standing: Standing) -> usize {
        self.reading
            .rows
            .iter()
            .filter(|row| row.group() != FieldGroup::Closed && row.frame.standing == standing)
            .count()
    }

    /// The run that has waited longest on a person, and the command that answers it.
    fn waiting_longest(&self, look: &Look) -> Option<Attention> {
        let row = *self.reading.needing_a_person().first()?;
        let command = row.command()?;
        let mut attention = Attention::command(row.said(look.glyphs), command);
        attention.subject = Some(row.frame.run_id.handle().to_string());
        Some(attention)
    }

    /// What `stroke` asks for in the field.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        self.said = None;
        match stroke {
            Stroke::Down | Stroke::Insert('j') => self.moved(1),
            Stroke::Up | Stroke::Insert('k') => self.moved(-1),
            Stroke::Enter => opened(self.pointed.clone()),
            Stroke::Insert('n') => opened(
                self.reading
                    .needing_a_person()
                    .first()
                    .map(|row| row.frame.run_id.clone()),
            ),
            Stroke::Insert(' ') => self.picked(),
            Stroke::Insert('r') => self.act(|run| Want::Work(Operation::Resume { run })),
            Stroke::Insert('c') => self.act(|run| {
                let asks = yunta_render::screen::cancelling(run.handle());
                let same = vec![NextStep::Cancel { run: run.clone() }];
                Want::Confirm(Confirmation::new(asks, same, Operation::Cancel { run }))
            }),
            Stroke::Insert('x') => self.closing(),
            Stroke::Insert('a') => {
                self.reach = match self.reach {
                    FieldReach::Here => FieldReach::Machine,
                    FieldReach::Machine => FieldReach::Here,
                };
                vec![Want::Work(Operation::Field { reach: self.reach })]
            }
            Stroke::Insert('d') => {
                self.depth = match self.depth.reaches(Depth::Full) {
                    true => Depth::Glance,
                    false => Depth::Full,
                };
                self.kept_to_listed();
                Vec::new()
            }
            Stroke::Insert('w') => vec![Want::Open(Stage::Catalog(CatalogStage::new()))],
            Stroke::Insert('m') => vec![Want::Open(Stage::Doctor(DoctorStage::new()))],
            Stroke::Insert('q') | Stroke::Decline | Stroke::Interrupt => vec![Want::Leave],
            _ => Vec::new(),
        }
    }

    /// What the field offers, each with the key that takes it here: following the run
    /// pointed at, resuming and cancelling it, closing what `x` closes where one command
    /// does it, the run waiting on the person, the field as far again or with its closed
    /// runs, the workflows and doctor.
    pub(crate) fn offered(&self) -> Vec<Act> {
        let on_the_run = self.pointed.iter().flat_map(|run| {
            [
                (Stroke::Enter, NextStep::Follow { run: run.clone() }),
                (Stroke::Insert('r'), NextStep::Resume { run: run.clone() }),
                (Stroke::Insert('c'), NextStep::Cancel { run: run.clone() }),
            ]
        });
        let closing = self
            .closed_by_one_command()
            .map(|run| (Stroke::Insert('x'), NextStep::Close { run }));
        let needs = (!self.reading.needing_a_person().is_empty())
            .then_some((Stroke::Insert('n'), NextStep::Needs));
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

    /// What the field asks for when a person comes back to it: itself, read again.
    pub(crate) fn returned(&self) -> Vec<Want> {
        vec![Want::Work(Operation::Field { reach: self.reach })]
    }

    /// What an answer to work the field asked for does: a field read again, or, after
    /// an act, the field asked again; what refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Field { reading, storage } => {
                self.reread(reading, storage);
                Vec::new()
            }
            Outcome::Closed {
                closed,
                refused,
                failed,
            } => {
                self.marked.retain(|run| !closed.contains(run));
                self.said = failed.or_else(|| refused.first().map(ToString::to_string));
                vec![Want::Work(Operation::Field { reach: self.reach })]
            }
            Outcome::Resumed { .. } | Outcome::Cancelled { .. } => {
                vec![Want::Work(Operation::Field { reach: self.reach })]
            }
            Outcome::Refused(refused) => {
                self.said = Some(refused.to_string());
                Vec::new()
            }
            Outcome::Failed(said) => {
                self.said = Some(said);
                Vec::new()
            }
            Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::Started { .. }
            | Outcome::Followed
            | Outcome::Page(_)
            | Outcome::NodePage(_)
            | Outcome::Questions(_)
            | Outcome::Decision(_)
            | Outcome::Stats(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// The pointer moved `by` rows, around at either end.
    fn moved(&mut self, by: isize) -> Vec<Want> {
        let listed = self.listed();
        if listed.is_empty() {
            return Vec::new();
        }
        let at = self
            .pointed
            .as_ref()
            .and_then(|run| listed.iter().position(|listed| listed == run))
            .unwrap_or(0);
        let to = (at as isize + by).rem_euclid(listed.len() as isize) as usize;
        self.pointed = Some(listed[to].clone());
        Vec::new()
    }

    /// The run pointed at picked, or put back.
    fn picked(&mut self) -> Vec<Want> {
        if let Some(run) = self.pointed.clone() {
            match self.marked.iter().position(|marked| *marked == run) {
                Some(at) => {
                    self.marked.remove(at);
                }
                None => self.marked.push(run),
            }
        }
        Vec::new()
    }

    /// `act` on the run pointed at, where there is one.
    fn act(&self, act: impl FnOnce(RunId) -> Want) -> Vec<Want> {
        self.pointed.clone().map(act).into_iter().collect()
    }

    /// The one run `x` closes, where it closes one: the run picked, or the one pointed
    /// at where none is. Several picked are closed together, which no one command does,
    /// so the palette offers that by its key alone.
    fn closed_by_one_command(&self) -> Option<RunId> {
        match self.marked.as_slice() {
            [] => self.pointed.clone(),
            [only] => Some(only.clone()),
            _ => None,
        }
    }

    /// Closing the runs picked, or the one pointed at where none is, asked first with
    /// every run it closes named.
    fn closing(&self) -> Vec<Want> {
        let runs = match self.marked.is_empty() {
            true => self.pointed.clone().into_iter().collect(),
            false => self.marked.clone(),
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

/// `run`'s stage opened over the field, where there is a run.
fn opened(run: Option<RunId>) -> Vec<Want> {
    run.map(|run| Want::Open(Stage::Run(RunStage::of(run))))
        .into_iter()
        .collect()
}
