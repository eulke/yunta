//! What keeps a plan from being proven as it is written, each flaw as a
//! caution: what is so, what it means, and what fixes it.

use yunta_core::shown::{Flaw, PlanReview};

use crate::blocks::{Concern, Section};
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};

/// `doc` with what keeps `review` from being proven, said before any
/// detail: a reader who goes on past it has been told.
pub(super) fn flawed(doc: Doc<'static>, review: &PlanReview) -> Doc<'static> {
    let flaws = review.flaws();
    if flaws.is_empty() {
        return doc;
    }
    doc.with(Section {
        mark: None,
        title: Line::new().push(
            Tone::Caution,
            "what keeps this plan from being proven as it is written",
        ),
        blocks: concerns(&flaws),
    })
}

/// Each of `flaws` as a caution.
pub(crate) fn concerns(flaws: &[Flaw]) -> Vec<Block<'static>> {
    flaws
        .iter()
        .map(|flaw| {
            Block::Concern(Concern {
                fact: flaw.to_string(),
                so: flaw.so(),
                fix: flaw.fix().to_string(),
            })
        })
        .collect()
}
