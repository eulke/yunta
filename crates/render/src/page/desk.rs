//! A plan as its desk reads it: the places a person can stand on, what each shows, and
//! what the rail says of the plan as a whole. Pure: where a person stands comes in, and
//! what stands in front of them goes out.
//!
//! No page: a terminal reads a plan here a place at a time, from the parts the plan's
//! page draws whole, and the desk finds that plan among what a decision shows, with
//! [`plan_among`], beside the documents it shows with it.

use surco::components::{Desk, DeskFoot, Pane, RailGroup, RailRow};
use surco::diagram::Diagram;
use surco::doc::Doc;
use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::text::counted;
use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
use yunta_core::{RunId, Task};

use super::plan::{card, change, head, proof, said, steps, tree, Form};
use super::{Page, Shown};

/// What else a decision shows beside its plan: a place of its own.
pub struct AlsoShown<'a> {
    /// The name the desk's rail and pane give it.
    pub name: String,
    /// What it shows, drawn whole in the pane, with the pane's look.
    pub shown: Shown<'a>,
    /// Whether the decision lists it before its plan, so it is read before it.
    pub leads: bool,
}

/// What is in front of a person on a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// What the place says: a task's card, or what the plan says of itself.
    Stop,
    /// What proves the place.
    Proof,
    /// What was decided for the person.
    Choices,
}

/// Where a person stands on a plan, and what they are looking at there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// The place, among the places of a [`Reading`].
    pub at: usize,
    /// What the person is looking at there.
    pub view: View,
    /// The first row of the pane, as the person has scrolled it.
    pub top: usize,
    /// The decision they are on, among [`Reading::choices`].
    pub choice: usize,
}

/// One place a person can stand on.
enum Place<'a> {
    /// What the plan says of itself.
    TheChange,
    /// A task, in its step, counted from 0, and its place among all the tasks.
    Task {
        task: &'a Task,
        step: usize,
        nth: usize,
    },
    /// What else the decision shows, by its place among [`Reading::also`].
    Also(usize),
}

/// A plan being read: its places in the order a person walks them.
pub struct Reading<'a> {
    review: &'a PlanReview,
    places: Vec<Place<'a>>,
    steps: Vec<Vec<&'a Task>>,
    also: Vec<AlsoShown<'a>>,
}

impl<'a> Reading<'a> {
    /// `review` as a person reads it, with what else its decision shows.
    pub fn of(review: &'a PlanReview, also: Vec<AlsoShown<'a>>) -> Self {
        let steps = review.plan.steps();
        let mut places: Vec<Place<'a>> = also
            .iter()
            .enumerate()
            .filter(|(_, also)| also.leads)
            .map(|(at, _)| Place::Also(at))
            .collect();
        if says_of_itself(review) {
            places.push(Place::TheChange);
        }
        let tasks = steps
            .iter()
            .enumerate()
            .flat_map(|(step, tasks)| tasks.iter().map(move |task| (step, *task)));
        places.extend(
            tasks
                .enumerate()
                .map(|(nth, (step, task))| Place::Task { task, step, nth }),
        );
        places.extend(
            also.iter()
                .enumerate()
                .filter(|(_, also)| !also.leads)
                .map(|(at, _)| Place::Also(at)),
        );
        Self {
            review,
            places,
            steps,
            also,
        }
    }

    /// How many places a person can stand on.
    pub fn stops(&self) -> usize {
        self.places.len()
    }

    /// How many decisions were made for the person.
    pub fn choices(&self) -> usize {
        change::decided_for_you(self.review).count()
    }

    /// Whether what was decided holds anything to show: a decision, an answer, a risk or
    /// what is left out.
    pub fn says_what_it_decided(&self) -> bool {
        let plan = &self.review.plan;
        !plan.decisions.is_empty()
            || change::answered(self.review).next().is_some()
            || !plan.risks.is_empty()
            || !plan.out_of_scope.is_empty()
    }

    /// Whether the place `at` has a proof to turn over to: a task does, and what the plan
    /// says of itself does where anything proves the plan as a whole.
    pub fn has_proof(&self, at: usize) -> bool {
        match self.places.get(at) {
            Some(Place::Task { .. }) => true,
            Some(Place::TheChange) => !proof::held(self.review, None).is_empty(),
            Some(Place::Also(_)) | None => false,
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
    /// measure, in the glyphs `look` draws with.
    pub fn pane_doc(&self, spot: &Spot, look: &Look) -> Doc {
        let Some(place) = self.places.get(spot.at) else {
            return Doc::new();
        };
        match (spot.view, place) {
            (View::Choices, _) => change::deciding(self.review, spot.choice, look.glyphs),
            (_, Place::Also(at)) => self
                .also
                .get(*at)
                .map_or_else(Doc::new, |also| also.shown.document(look)),
            (View::Proof, Place::Task { task, .. }) => proof::of_task(task, self.review),
            (View::Proof, Place::TheChange) => proof::of_plan(self.review),
            (View::Stop, Place::Task { task, .. }) => {
                Doc::new().with_all(card::card(task, self.review).blocks)
            }
            (View::Stop, Place::TheChange) => change::words(self.review),
        }
    }

    /// The desk at `spot`. `title` is what opens its first row, to which the plan's
    /// size is added; `edge` is what stands at the far end of it.
    pub fn desk(
        &self,
        spot: &Spot,
        title: &[String],
        edge: Option<String>,
        foot: DeskFoot,
        look: &Look,
    ) -> Desk {
        let review = self.review;
        let size = head::size(review, self.steps.len());
        Desk {
            title: title.iter().cloned().chain([size]).collect(),
            edge,
            gist: said(&review.plan.summary).map(str::to_string),
            asked: said(&review.asked).map(str::to_string),
            rail: self.rail(spot),
            pane: self.pane(spot, look),
            foot,
        }
    }

    /// The groups of the rail that name the places: what is read first, what the plan says
    /// of itself, each task in its step, and what else is shown.
    fn rail_of_places(&self, spot: &Spot) -> Vec<RailGroup> {
        let read_first = self.stop_rows(spot, |place| match place {
            Place::Also(at) => self
                .also
                .get(*at)
                .filter(|also| also.leads)
                .map(|also| (String::new(), also.name.clone())),
            _ => None,
        });
        let change = self.stop_rows(spot, |place| {
            matches!(place, Place::TheChange).then(|| (String::new(), "the change".to_string()))
        });
        let tasks = self.stop_rows(spot, |place| match place {
            Place::Task { task, step, .. } => {
                let first = self.steps[*step]
                    .first()
                    .is_some_and(|first| first.id == task.id);
                let lead = if first {
                    (step + 1).to_string()
                } else {
                    String::new()
                };
                Some((lead, task.id.as_str().to_string()))
            }
            _ => None,
        });
        let also = self.stop_rows(spot, |place| match place {
            Place::Also(at) => self
                .also
                .get(*at)
                .filter(|also| !also.leads)
                .map(|also| (String::new(), also.name.clone())),
            _ => None,
        });
        [
            group(Some("read first"), read_first),
            group(None, change),
            group(Some("steps"), tasks),
            group(Some("also shown"), also),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The groups of the rail that say the plan as a whole: what keeps it from being
    /// proven, its order, what was decided and what it leaves out, what it implies beyond
    /// its tasks, and what each task changes.
    fn rail_of_plan(&self) -> Vec<RailGroup> {
        let review = self.review;
        let plan = &review.plan;
        let decided = change::decided_for_you(review).count();
        let tallies: Vec<RailRow> = [
            ("decided for you", decided),
            ("risks", plan.risks.len()),
            ("left out", plan.out_of_scope.len()),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(what, count)| RailRow::Tally {
            what: what.to_string(),
            count,
            key: Some('d'),
        })
        .collect();
        let implies: Vec<RailRow> = tree::implied_by_all(&plan.shapes, review.shapes_read.as_ref())
            .into_iter()
            .map(RailRow::Weighs)
            .collect();
        [
            group(
                None,
                head::flawed(review)
                    .map(RailRow::Weighs)
                    .into_iter()
                    .collect(),
            ),
            group(
                Some("order"),
                self.order().into_iter().map(RailRow::Order).collect(),
            ),
            group(None, tallies),
            group(Some("implies"), implies),
            group(Some("impact"), self.impact()),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The stops `lead_and_name` gives each place, as the rows a rail draws: each one
    /// pointed when `spot` stands on its place, and not while a person is choosing.
    fn stop_rows(
        &self,
        spot: &Spot,
        lead_and_name: impl Fn(&Place<'a>) -> Option<(String, String)>,
    ) -> Vec<RailRow> {
        self.places
            .iter()
            .enumerate()
            .filter_map(|(at, place)| {
                let (lead, name) = lead_and_name(place)?;
                Some(RailRow::Stop {
                    lead,
                    name,
                    pointed: spot.view != View::Choices && spot.at == at,
                })
            })
            .collect()
    }

    /// The tasks in the order the steps hold them.
    fn tasks(&self) -> impl Iterator<Item = &'a Task> + '_ {
        self.steps.iter().flatten().copied()
    }

    /// The order the tasks wait on each other in, drawn over their short names.
    fn order(&self) -> Option<Diagram> {
        let names = self.task_names();
        let labels: Vec<&str> = names.iter().map(String::as_str).collect();
        steps::order(&self.steps, &labels)
    }

    /// Each task's short name, in the order the steps hold the tasks.
    fn task_names(&self) -> Vec<String> {
        let ids: Vec<&str> = self.tasks().map(|task| task.id.as_str()).collect();
        steps::short_names(&ids)
    }

    /// What each task changes, a row a path each, with the task's short name beside it.
    fn impact(&self) -> Vec<RailRow> {
        let names = self.task_names();
        self.tasks()
            .zip(names)
            .flat_map(|(task, name)| {
                let judged = card::judged(self.review, task);
                let files = card::changed_files(task, judged);
                card::impact_paths(&files)
                    .into_iter()
                    .map(move |left| RailRow::Pair {
                        left,
                        right: name.clone(),
                    })
            })
            .collect()
    }

    /// The pane of a desk at `spot`: its title, what is at its far end, where it is,
    /// what it holds, and whether the row a pointer stands on stays in sight.
    fn pane(&self, spot: &Spot, look: &Look) -> Pane {
        let (title, edge, place) = self.heading(spot, look.glyphs.sep());
        Pane {
            title,
            edge,
            place,
            doc: self.pane_doc(spot, look),
            top: spot.top,
            follows_pointer: spot.view == View::Choices,
        }
    }

    /// The pane's title, what stands at its far end and where it is, as a person at
    /// `spot` reads them. `sep` joins the parts of a proof's title.
    fn heading(&self, spot: &Spot, sep: char) -> (String, Option<String>, Option<String>) {
        let place = self.places.get(spot.at);
        match (spot.view, place) {
            (View::Choices, _) => match change::decided_for_you(self.review).count() {
                0 => ("what was decided".to_string(), None, None),
                n => (
                    "decided for you".to_string(),
                    None,
                    Some(counted(n, "choice")),
                ),
            },
            (view, Some(Place::Task { task, step, nth })) => {
                let edge = format!("step {} of {}", step + 1, self.steps.len());
                let place = format!("{}, task {} of {}", task.id, nth + 1, self.tasks().count());
                let title = match view {
                    View::Proof => format!("proof {sep} {}", task.id),
                    _ => task.title.clone(),
                };
                (title, Some(edge), Some(place))
            }
            (View::Proof, Some(Place::TheChange)) => {
                (format!("proof {sep} the whole plan"), None, None)
            }
            (_, Some(Place::TheChange)) => ("the change".to_string(), None, None),
            (_, Some(Place::Also(at))) => {
                let name = self
                    .also
                    .get(*at)
                    .map_or_else(String::new, |also| also.name.clone());
                (name, None, None)
            }
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

/// The lines a decision's card says of `review`: what it is about, unless `question` says
/// it; how big it is, a fact each; and what it implies past its tasks.
pub fn about(review: &PlanReview, question: &str, glyphs: Glyphs) -> Vec<String> {
    let steps = review.plan.steps();
    let mut lines: Vec<String> = Vec::new();
    if let Some(summary) = said(&review.plan.summary).filter(|summary| !question.contains(*summary))
    {
        lines.push(summary.to_string());
    }
    lines.push(head::facts(review, steps.len()).join(&format!(" {} ", glyphs.sep())));
    lines.extend(tree::implied_by_all(
        &review.plan.shapes,
        review.shapes_read.as_ref(),
    ));
    lines
}

/// The plan among `documents` as a person reads it, with the others as what else it
/// shows; `None` when no document is a plan, or the plan has no place to stand on.
pub fn reading<'a>(documents: &'a [ShownDocument], run: &'a RunId) -> Option<Reading<'a>> {
    let (plan, review) = plan_among(documents)?;
    let mut leads = true;
    let mut also = Vec::new();
    for document in documents {
        if std::ptr::eq(document, plan) {
            leads = false;
            continue;
        }
        also.push(AlsoShown {
            name: super::shown::called(document),
            shown: Shown {
                document,
                run,
                form: Form::Review,
            },
            leads,
        });
    }
    Some(Reading::of(review, also))
}

/// Whether `review` holds a place for a person to stand on: a task, or what it says of
/// itself. A plan with neither has nothing to read at a desk.
fn has_a_place(review: &PlanReview) -> bool {
    !review.plan.tasks.is_empty() || says_of_itself(review)
}

/// Whether the plan says anything of itself as a whole.
fn says_of_itself(review: &PlanReview) -> bool {
    said(&review.plan.description).is_some() || said(&review.plan.design).is_some()
}

/// The first document among `documents` that is a plan with something to read,
/// with the plan it holds: what a person reads at a desk. A plan with no task,
/// description or design is a document like any other.
pub fn plan_among(documents: &[ShownDocument]) -> Option<(&ShownDocument, &PlanReview)> {
    documents
        .iter()
        .find_map(|document| match &document.content {
            ShownContent::Tasks(review) if has_a_place(review) => Some((document, &**review)),
            _ => None,
        })
}
