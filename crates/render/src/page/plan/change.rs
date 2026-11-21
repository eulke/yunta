//! What a plan changes, said before how it is proven: what it does, in its
//! author's words and drawings, every choice it made for the person, with
//! what they answered beside one that restates it, what it leaves out, what
//! it risks where the whole plan is shown, and how it is built; and what was
//! accepted departing from it.
//!
//! The choices are [`decided_for_you`]'s, and how many there are is
//! [`how_many_decided`]'s: the printed page and its head, a decision's card,
//! the desk's view and its rail read them there, so a plan's choices are
//! listed and counted one way wherever they are said.

use surco::components::{Fields, Lead, Marked, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use yunta_core::events::AcceptedDeparture;
use yunta_core::shown::{Answered, PlanReview};
use yunta_core::Decision;

use super::{blank, said, titled, Form};

/// `doc` with what `review`'s plan changes, as `form` shows it: its
/// description whole, every diagram in it drawn; every decision it made for
/// the person, numbered in the plan's order; what it leaves out and, where
/// the whole plan is shown, what it risks; and its design.
pub(super) fn change(mut doc: Doc, review: &PlanReview, form: Form) -> Doc {
    let plan = &review.plan;
    if let Some(description) = said(&plan.description) {
        doc = doc
            .with(blank())
            .with(Block::Markdown(description.to_string()));
    }
    let choices: Vec<Block> = decided_for_you(review)
        .enumerate()
        .map(|(at, (decision, answered))| decided(at + 1, decision, answered).into())
        .collect();
    if !choices.is_empty() {
        doc = doc.with(titled("decided for you", choices));
    }
    // Each risk is said beside the decision a page puts the plan under, so
    // only the whole plan draws them here.
    let risks: &[String] = match form {
        Form::Whole => plan.risks.as_slice(),
        Form::Review | Form::Published | Form::Browser => &[],
    };
    for (title, mark, items) in [
        ("risks", Mark::Caution, risks),
        ("left out", Mark::Absent, plan.out_of_scope.as_slice()),
    ] {
        if !items.is_empty() {
            doc = doc.with(titled(
                title,
                vec![Marked {
                    mark,
                    items: items.to_vec(),
                }
                .into()],
            ));
        }
    }
    if let Some(design) = said(&plan.design) {
        doc = doc.with(titled("design", vec![Block::Markdown(design.to_string())]));
    }
    doc
}

/// The decisions the plan made for the person, in the plan's order: every one, since each is
/// the plan's to keep, and beside one that restates what the person answered, their answer.
pub(in crate::page) fn decided_for_you(
    review: &PlanReview,
) -> impl Iterator<Item = (&Decision, Option<&Answered>)> {
    review
        .plan
        .decisions
        .iter()
        .map(|decision| (decision, answer_of(review, decision)))
}

/// How many decisions the plan made for the person: each [`decided_for_you`] lists, as the
/// head of its page, a decision's card, its desk's rail and the desk's view of them count
/// them.
pub(in crate::page) fn how_many_decided(review: &PlanReview) -> usize {
    decided_for_you(review).count()
}

/// What a person answered to the question `decision` restates, when they
/// answered it.
fn answer_of<'a>(review: &'a PlanReview, decision: &Decision) -> Option<&'a Answered> {
    let id = decision.answers.as_ref()?;
    review.answered.iter().find(|answered| answered.id == *id)
}

/// What the plan says of itself as a whole, as a desk reads it under a pane
/// titled `the change`: its description, then its design, a paragraph after it
/// with no heading of its own, every diagram in either drawn.
pub(in crate::page) fn words(review: &PlanReview) -> Doc {
    [&review.plan.description, &review.plan.design]
        .into_iter()
        .filter_map(said)
        .fold(Doc::new(), |doc, words| {
            doc.with(Block::Markdown(words.to_string()))
        })
}

/// What the plan decided for the person, as its desk reads it: every decision it made, those
/// that restate what the person answered among them, each its question with what the plan
/// chose under it and why after that, in the order the plan made them. What it leaves out and
/// what it risks the desk's rail counts, and the printed page says whole.
pub(in crate::page) fn decided_at_the_desk(review: &PlanReview) -> Doc {
    decided_for_you(review).fold(Doc::new(), |doc, (decision, _)| {
        let why = said(&decision.why).map(|why| Block::Markdown(why.to_string()));
        doc.with(Block::Heading(decision.question.clone()))
            .with(Block::Markdown(decision.choice.clone()))
            .with_all(why)
    })
}

/// What was open, as the plan answers it: what it chose, instead of what, and why; then,
/// where it restates what the person `answered`, the question as they were asked it, where
/// its words are not the decision's own, and their answer.
fn reasons(decision: &Decision, answered: Option<&Answered>) -> Fields {
    let mut fields = Fields::new().push_if("chose", decision.choice.as_str());
    for (n, alternative) in decision.alternatives.iter().enumerate() {
        fields = fields.push_if(if n == 0 { "instead of" } else { "" }, alternative.as_str());
    }
    if let Some(why) = said(&decision.why) {
        fields = fields.push_if("because", why);
    }
    if let Some(answered) = answered {
        // The decision's question titles its entry: the page says a question once.
        let asked = answered.question.trim();
        if asked != decision.question.trim() {
            fields = fields.push_if("asked", asked);
        }
        fields = fields.push_if("you answered", answered.answer.as_str());
    }
    fields
}

/// The decision `at` in the plan's order: what was open, what the plan chose instead of what,
/// and why, with what the person was asked and `answered` where it restates their answer.
fn decided(at: usize, decision: &Decision, answered: Option<&Answered>) -> Section {
    Section {
        lead: Some(Lead::Ordinal(at)),
        title: Line::new().push(Tone::Strong, decision.question.as_str()),
        blocks: vec![reasons(decision, answered).into()],
    }
}

/// Every departure from the plan a person accepted: what the plan says,
/// what was built instead and why. Where the work stops being what the
/// plan says, which a reader after the work meets first.
pub(super) fn departures(departed: &[AcceptedDeparture]) -> Section {
    let blocks = departed
        .iter()
        .map(|departure| {
            let declared = &departure.declared;
            let mut fields = Fields::new()
                .push_if("the plan", declared.planned.as_str())
                .push_if("built", declared.instead.as_str())
                .push_if("because", declared.why.as_str());
            if let Some(said) = said(&departure.said) {
                fields = fields.push_if("accepted", said);
            }
            Section {
                lead: None,
                title: Line::new().plain(format!(
                    "{} departs from {}",
                    declared.task_id, declared.from
                )),
                blocks: vec![fields.into()],
            }
            .into()
        })
        .collect();
    titled("accepted departures from this plan", blocks)
}
