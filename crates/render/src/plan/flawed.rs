//! What keeps a plan from being proven as it is written, each flaw as a
//! caution: what is so, what it means, and what fixes it.

use surco::components::{Concern, Section};
use surco::doc::{Block, Doc};
use surco::line::{Line, Tone};
use yunta_core::shown::{Flaw, PlanReview};

/// `doc` with what keeps `review` from being proven, said before any
/// detail: a reader who goes on past it has been told.
pub(super) fn flawed(doc: Doc, review: &PlanReview) -> Doc {
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
pub(crate) fn concerns(flaws: &[Flaw]) -> Vec<Block> {
    flaws
        .iter()
        .map(|flaw| {
            Block::Concern(Concern {
                fact: flaw.to_string(),
                so: flaw.so(),
                fix: surco::components::Fix::said(flaw.fix()),
            })
        })
        .collect()
}
