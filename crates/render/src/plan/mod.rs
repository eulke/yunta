//! A plan as a person reads it: what it changes before how it is proven.
//! It opens with what the plan is and what the person asked for, and with
//! every departure from it a person accepted; then the change — what it
//! does, what it decided for them and what they answered, what it leaves
//! out, what it risks where the whole plan is shown, and how it is
//! designed; then the steps it takes, and each task's card with the code
//! it writes; then what proves it.
//!
//! A person deciding on a change reads the change. The code a task writes
//! sits in its card, once; the code of the tests that prove it is not part
//! of the read — the whole plan has each file the spec wrote after every
//! card, and a review names the command that shows them.

use surco::components::{Code, Fold, Section};
use surco::doc::{Block, Doc};
use surco::line::{Line, Tone};
use yunta_core::shown::PlanReview;
use yunta_core::step::NextStep;
use yunta_core::RunId;

mod card;
mod change;
mod desk;
mod flawed;
mod head;
mod proof;
mod steps;
mod tree;

pub(crate) use desk::has_a_place;
pub use desk::{about, reading, AlsoShown, Reading, Spot, View};

pub(crate) use card::codeless;
pub(crate) use flawed::concerns;

/// How much of a plan a reader is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// At a terminal, where a decision is made on it: the change, and what
    /// proves each task in a line; no test's code, the command that shows
    /// the spec's tests whole, and its risks said beside the decision.
    Review,
    /// On a forge's page, where a decision is made on it: what a review
    /// shows, each card and the proof folded, and no command that shows the
    /// spec's tests whole, since the whole plan is published beside it.
    Published,
    /// In a browser, where a decision is made on it and there is room: what a
    /// review shows, and each file the spec wrote, whole, after the cards, with
    /// no command to show them, since a page runs none; its risks said beside
    /// the decision.
    Browser,
    /// Everything the plan says, and each file the spec wrote, whole, after
    /// the cards.
    Whole,
}

/// `review` as a document, under the headline of the page that reads it.
/// `run` is the run it is read in; each command it names is written by
/// `spell`.
pub fn document(
    review: &PlanReview,
    run: &RunId,
    form: Form,
    spell: &dyn Fn(&NextStep) -> String,
) -> Doc {
    let steps = review.plan.steps();
    let mut doc = head::head(review, &steps);
    if !review.departed.is_empty() {
        doc = doc.with(change::departures(&review.departed));
    }
    let doc = change::change(doc, review, form);
    let doc = steps::steps(doc, &steps);
    let total = steps.len();
    let doc = steps.iter().enumerate().fold(doc, |doc, (at, step)| {
        let cards: Vec<Block> = step
            .iter()
            .map(|task| card_block(card::card(task, review), form))
            .collect();
        doc.with(titled(&format!("step {} of {total}", at + 1), cards))
    });
    let doc = match form {
        Form::Review | Form::Published => doc,
        Form::Browser | Form::Whole => tests_whole(doc, review),
    };
    proof::proof(doc, review, form, (run, spell))
}

/// `card` as a block of its step: a forge's page folds each card under its
/// title, so a reader opens the ones they want; every other form draws it
/// open.
fn card_block(card: Section, form: Form) -> Block {
    match form {
        Form::Published => Fold(card).into(),
        Form::Review | Form::Browser | Form::Whole => card.into(),
    }
}

/// Each file the spec wrote, whole, after every card, with the task its
/// tests prove: what the whole plan holds that a review leaves a command
/// away.
fn tests_whole(doc: Doc, review: &PlanReview) -> Doc {
    let files: Vec<Block> = review
        .spec
        .iter()
        .flat_map(|spec| &spec.specs)
        .flat_map(|spec| {
            spec.files.iter().map(move |file| {
                let what = format!("what proves `{}`", spec.task);
                Code::whole(file.path.as_str(), Some(what), &file.content).into()
            })
        })
        .collect();
    match files.is_empty() {
        true => doc,
        false => doc.with(titled("the spec's tests", files)),
    }
}

/// A blank line between two blocks a reader takes apart.
pub(super) fn blank() -> Block {
    Block::Lines(vec![Line::new()])
}

/// A section titled `title`, in the plan's own heading tone.
pub(super) fn titled(title: &str, blocks: Vec<Block>) -> Section {
    Section {
        lead: None,
        title: Line::new().push(Tone::Strong, title),
        blocks,
    }
}

/// Text that says something, trimmed.
pub(super) fn said(text: &Option<String>) -> Option<&str> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}
