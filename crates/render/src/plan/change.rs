//! What a plan changes, said before how it is proven: what it does, in its
//! author's words and drawings, the choices it made for the person and
//! what they answered, what it leaves out, what it risks where the whole
//! plan is shown, and how it is built; and what was accepted departing
//! from it.

use surco::components::{Fields, Lead, Marked, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use yunta_core::events::AcceptedDeparture;
use yunta_core::shown::{Answered, PlanReview};
use yunta_core::Decision;

use super::{blank, said, titled, Form};

/// `doc` with what `review`'s plan changes, as `form` shows it: its
/// description whole, every diagram in it drawn; its decisions; what it
/// leaves out and, where the whole plan is shown, what it risks; and its
/// design.
pub(super) fn change(mut doc: Doc, review: &PlanReview, form: Form) -> Doc {
    let plan = &review.plan;
    if let Some(description) = said(&plan.description) {
        doc = doc
            .with(blank())
            .with(Block::Markdown(description.to_string()));
    }
    let ours: Vec<Block> = decided_for_you(review)
        .enumerate()
        .map(|(at, decision)| decided(at + 1, decision).into())
        .collect();
    if !ours.is_empty() {
        doc = doc.with(titled("decided for you", ours));
    }
    let yours: Vec<Block> = answered(review)
        .enumerate()
        .map(|(at, (decision, answered))| restated(at + 1, decision, answered).into())
        .collect();
    if !yours.is_empty() {
        doc = doc.with(titled("you answered", yours));
    }
    // Each risk is said beside the decision a page puts the plan under, so
    // only the whole plan draws them here.
    let risks: &[String] = match form {
        Form::Whole => plan.risks.as_slice(),
        Form::Review | Form::Published => &[],
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

/// The decisions the plan made for the person: each that restates no
/// answer of theirs.
pub(super) fn decided_for_you(review: &PlanReview) -> impl Iterator<Item = &Decision> {
    review
        .plan
        .decisions
        .iter()
        .filter(|decision| answer_of(review, decision).is_none())
}

/// Each decision that restates what a person answered, with the answer.
fn answered(review: &PlanReview) -> impl Iterator<Item = (&Decision, &Answered)> {
    review
        .plan
        .decisions
        .iter()
        .filter_map(|decision| Some((decision, answer_of(review, decision)?)))
}

/// What a person answered to the question `decision` restates, when they
/// answered it.
fn answer_of<'a>(review: &'a PlanReview, decision: &Decision) -> Option<&'a Answered> {
    let id = decision.answers.as_ref()?;
    review.answered.iter().find(|answered| answered.id == *id)
}

/// One decision: what was open, what the plan chose instead of what, and
/// why.
fn decided(at: usize, decision: &Decision) -> Section {
    let mut fields = Fields::new().push_if("chose", decision.choice.as_str());
    for (n, alternative) in decision.alternatives.iter().enumerate() {
        fields = fields.push_if(if n == 0 { "instead of" } else { "" }, alternative.as_str());
    }
    if let Some(why) = said(&decision.why) {
        fields = fields.push_if("because", why);
    }
    Section {
        lead: Some(Lead::Ordinal(at)),
        title: Line::new().push(Tone::Strong, decision.question.as_str()),
        blocks: vec![fields.into()],
    }
}

/// A decision that restates what a person answered: their words, and
/// what the plan does with them.
fn restated(at: usize, decision: &Decision, answered: &Answered) -> Section {
    Section {
        lead: Some(Lead::Ordinal(at)),
        title: Line::new().push(Tone::Strong, answered.question.as_str()),
        blocks: vec![Fields::new()
            .push_if("you said", answered.answer.as_str())
            .push_if("the plan", decision.choice.as_str())
            .into()],
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
