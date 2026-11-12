//! See [`super`]. The plan among what a decision shows, read for its desk: the first
//! document that is a plan with a place to stand on, the reading of it with the other
//! documents as what else it shows, and what a decision's card says of it.

use surco::glyphs::Glyphs;
use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
use yunta_core::RunId;

use super::super::plan::{head, said, tree, Form};
use super::{says_of_itself, AlsoShown, Reading};

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

/// The plan among `documents`, the run `run`'s, as a person reads it, with the others
/// as what else it shows; `None` when no document is a plan, or the plan has no place to
/// stand on.
pub fn reading(documents: &[ShownDocument], run: &RunId) -> Option<Reading> {
    let (plan, review) = plan_among(documents)?;
    let mut leads = true;
    let mut also = Vec::new();
    for document in documents {
        if std::ptr::eq(document, plan) {
            leads = false;
            continue;
        }
        also.push(AlsoShown {
            name: super::super::shown::called(document),
            document: document.clone(),
            run: run.clone(),
            form: Form::Review,
            leads,
        });
    }
    Some(Reading::of(review.clone(), also))
}

/// Whether `review` holds a place for a person to stand on: a task, or what it says of
/// itself. A plan with neither has nothing to read at a desk.
fn has_a_place(review: &PlanReview) -> bool {
    !review.plan.tasks.is_empty() || says_of_itself(review)
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
