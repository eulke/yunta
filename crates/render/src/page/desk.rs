//! A plan as its desk reads it: the places a person can stand on, what each shows, and
//! what the rail says of the plan as a whole. Pure: where a person stands comes in, and
//! what stands in front of them goes out.
//!
//! No page: a terminal reads a plan here a place at a time, from the parts the plan's
//! page draws whole, and the desk finds that plan among what a decision shows, with
//! [`plan_among`]. Its places are the plan's own: what it says of itself as a whole, and
//! each task. What else the decision shows is read on the printed page, and above a card
//! that stands in place of the desk.

use surco::components::{Desk, DeskFoot, Pane, RailGroup, RailRow, Sight};
use surco::doc::Doc;
use surco::look::Look;
use surco::text::{counted, counted_as};
use yunta_core::shown::PlanReview;
use yunta_core::Task;

use super::plan::{card, change, head, proof, said};

mod among;
pub use among::{about, plan_among, reading};

/// What is in front of a person on a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// What the place says: a task's card, or what the plan says of itself.
    Stop,
    /// What proves the place.
    Proof,
    /// What the plan decided for the person, whatever place they stand on.
    Choices,
}

/// Where a person stands on a plan, and what they are looking at there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// The place, among the places of a [`Reading`].
    pub at: usize,
    /// What the person is looking at there.
    pub view: View,
}

/// One place a person can stand on.
#[derive(Debug, Clone)]
enum Place {
    /// What the plan says of itself.
    TheChange,
    /// A task, by its place among the plan's tasks, in its step, counted from 0.
    Task { at: usize, step: usize },
}

/// A plan being read: its places in the order a person walks them. It owns what it
/// reads, so whatever holds a plan's desk holds it whole.
#[derive(Debug, Clone)]
pub struct Reading {
    review: PlanReview,
    places: Vec<Place>,
    /// Each step's tasks, by their places among the plan's tasks.
    steps: Vec<Vec<usize>>,
}

impl Reading {
    /// `review` as a person reads it: what it says of itself first, where it says anything,
    /// then each task in the order its steps hold them.
    pub fn of(review: PlanReview) -> Self {
        let steps: Vec<Vec<usize>> = review
            .plan
            .steps()
            .iter()
            .map(|tasks| {
                tasks
                    .iter()
                    .filter_map(|task| place_of(&review, task))
                    .collect()
            })
            .collect();
        let mut places: Vec<Place> = Vec::new();
        if says_of_itself(&review) {
            places.push(Place::TheChange);
        }
        let tasks = steps
            .iter()
            .enumerate()
            .flat_map(|(step, tasks)| tasks.iter().map(move |at| (step, *at)));
        places.extend(tasks.map(|(step, at)| Place::Task { at, step }));
        Self {
            review,
            places,
            steps,
        }
    }

    /// The plan's task at `at`, among its tasks.
    fn task(&self, at: usize) -> &Task {
        &self.review.plan.tasks[at]
    }

    /// How many places a person can stand on.
    pub fn stops(&self) -> usize {
        self.places.len()
    }

    /// The place of the plan's first task, among the places a person stands on: none for a
    /// plan with no task.
    pub fn first_task(&self) -> Option<usize> {
        self.places
            .iter()
            .position(|place| matches!(place, Place::Task { .. }))
    }

    /// Whether the plan says what it decided for the person: it made a decision.
    pub fn says_what_it_decided(&self) -> bool {
        change::how_many_decided(&self.review) > 0
    }

    /// Whether the place `at` has a proof to turn over to: a task does, and what the plan
    /// says of itself does where anything proves the plan as a whole.
    pub fn has_proof(&self, at: usize) -> bool {
        match self.places.get(at) {
            Some(Place::Task { .. }) => true,
            Some(Place::TheChange) => !proof::held(&self.review, None).is_empty(),
            None => false,
        }
    }

    /// What the rail says to a person at `spot`: the places first, then what the plan
    /// says of itself as a whole.
    pub fn rail(&self, spot: &Spot) -> Vec<RailGroup> {
        let mut groups = self.rail_of_places(spot);
        groups.extend(self.rail_of_plan());
        groups
    }

    /// What the pane holds at `spot`, as a document a surface lays out at the pane's
    /// measure.
    pub fn pane_doc(&self, spot: &Spot) -> Doc {
        let Some(place) = self.places.get(spot.at) else {
            return Doc::new();
        };
        match (spot.view, place) {
            (View::Choices, _) => change::decided_at_the_desk(&self.review),
            (View::Proof, Place::Task { at, .. }) => proof::of_task(self.task(*at), &self.review),
            (View::Proof, Place::TheChange) => proof::of_plan(&self.review),
            (View::Stop, Place::Task { at, .. }) => {
                Doc::new().with_all(card::at_its_desk(self.task(*at), &self.review))
            }
            (View::Stop, Place::TheChange) => change::words(&self.review),
        }
    }

    /// The desk at `spot`, its pane from the line `top` a person scrolled it to. `title`
    /// is what opens its first row, to which the plan's size is added; `edge` is what
    /// stands at the far end of it.
    pub fn desk(
        &self,
        spot: &Spot,
        top: usize,
        title: &[String],
        edge: Option<String>,
        foot: DeskFoot,
        look: &Look,
    ) -> Desk {
        let review = &self.review;
        Desk {
            title: title.iter().cloned().chain([self.size()]).collect(),
            edge,
            gist: said(&review.plan.summary).map(str::to_string),
            asked: said(&review.asked).map(str::to_string),
            rail: self.rail(spot),
            pane: self.pane(spot, top, look),
            foot,
        }
    }

    /// How big the plan is, as its desk's head says it: its tasks, and the steps they take
    /// where they take more than one; then the files it names for its work, where it names
    /// any.
    fn size(&self) -> String {
        let tasks = head::size(&self.review, self.steps.len());
        match head::files_named(&self.review) {
            0 => tasks,
            files => format!("{tasks}, {}", counted(files, "file")),
        }
    }

    /// The places of the rail, in the order a person walks them and in one untitled group:
    /// what the plan says of itself, then each task with its step's number on the step's first
    /// task. The place `spot` stands on is pointed, whatever the pane shows of it.
    fn rail_of_places(&self, spot: &Spot) -> Vec<RailGroup> {
        let stops = self
            .places
            .iter()
            .enumerate()
            .map(|(at, place)| {
                let (lead, name) = self.stop(place);
                RailRow::Stop {
                    lead,
                    name,
                    pointed: spot.at == at,
                }
            })
            .collect();
        group(None, stops).into_iter().collect()
    }

    /// What the rail names `place` by, after what leads it: a task, its step's number when
    /// it is the step's first.
    fn stop(&self, place: &Place) -> (String, String) {
        match place {
            Place::TheChange => (String::new(), "the change".to_string()),
            Place::Task { at, step } => {
                let lead = match self.steps[*step].first() == Some(at) {
                    true => (step + 1).to_string(),
                    false => String::new(),
                };
                (lead, self.task(*at).id.as_str().to_string())
            }
        }
    }

    /// The groups of the rail that say the plan as a whole: what it says, a count a row, the
    /// key that opens what was decided for the person beside its count; and what keeps it
    /// from being proven. What it implies, the order of its tasks and what each changes are
    /// said by its cards and its printed page.
    fn rail_of_plan(&self) -> Vec<RailGroup> {
        let review = &self.review;
        let plan = &review.plan;
        let tallies: Vec<RailRow> = [
            (
                "decided for you",
                change::how_many_decided(review),
                Some('d'),
            ),
            ("out of scope", plan.out_of_scope.len(), None),
            ("risks", plan.risks.len(), None),
        ]
        .into_iter()
        .filter(|(_, count, _)| *count > 0)
        .map(|(what, count, key)| RailRow::Tally {
            what: what.to_string(),
            count,
            key,
        })
        .collect();
        [
            group(Some("what it says"), tallies),
            group(
                Some("what weighs on it"),
                head::flawed(review)
                    .map(RailRow::Weighs)
                    .into_iter()
                    .collect(),
            ),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The tasks in the order the steps hold them.
    fn tasks(&self) -> impl Iterator<Item = &Task> + '_ {
        self.steps.iter().flatten().map(|at| self.task(*at))
    }

    /// The pane of a desk at `spot`, from the line `top`: its title, what is at its far
    /// end, where it is, and what it holds, read from its top.
    fn pane(&self, spot: &Spot, top: usize, look: &Look) -> Pane {
        let (title, edge, place) = self.heading(spot, look.glyphs.sep());
        Pane {
            title,
            edge,
            place,
            doc: self.pane_doc(spot),
            sight: Sight::from_line(top),
        }
    }

    /// The pane's title, what stands at its far end and where it is, as a person at
    /// `spot` reads them: what the plan decided for them with how many choices, which is no
    /// place; the change with how many tasks it holds, a task by its id with its step, and a
    /// task's proof with how many criteria it is held to; where it is, its place among the
    /// places a person stands on, `2 of 2`, which a rail says itself where one stands. `sep`
    /// joins the parts of the whole plan's proof's title.
    fn heading(&self, spot: &Spot, sep: char) -> (String, Option<String>, Option<String>) {
        let place = self.places.get(spot.at);
        let among = Some(format!("{} of {}", spot.at + 1, self.places.len()));
        match (spot.view, place) {
            (View::Choices, _) => (
                "decided for you".to_string(),
                Some(counted(change::how_many_decided(&self.review), "choice")),
                None,
            ),
            (View::Proof, Some(Place::Task { at, .. })) => {
                let task = self.task(*at);
                let criteria = counted_as(task.criteria.len(), "criterion", "criteria");
                (format!("{}, its proof", task.id), Some(criteria), among)
            }
            (View::Stop, Some(Place::Task { at, step })) => {
                let step = format!("step {} of {}", step + 1, self.steps.len());
                (self.task(*at).id.to_string(), Some(step), among)
            }
            (View::Proof, Some(Place::TheChange)) => {
                (format!("proof {sep} the whole plan"), None, among)
            }
            (View::Stop, Some(Place::TheChange)) => (
                "the change".to_string(),
                Some(counted(self.tasks().count(), "task")),
                among,
            ),
            (_, None) => (String::new(), None, None),
        }
    }
}

/// The rail's group titled `title` with `rows`, when it holds any rows.
fn group(title: Option<&str>, rows: Vec<RailRow>) -> Option<RailGroup> {
    (!rows.is_empty()).then(|| RailGroup {
        title: title.map(str::to_string),
        rows,
    })
}

/// Where `task`, one of `review`'s tasks, stands among them.
fn place_of(review: &PlanReview, task: &Task) -> Option<usize> {
    review
        .plan
        .tasks
        .iter()
        .position(|held| std::ptr::eq(held, task))
}

/// Whether the plan says anything of itself as a whole.
pub(super) fn says_of_itself(review: &PlanReview) -> bool {
    said(&review.plan.description).is_some() || said(&review.plan.design).is_some()
}
