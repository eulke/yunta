//! What a plan is, said first: the plan, how big it is and what it holds
//! for its reader, what it does, what keeps it from being proven, and what
//! the person asked for.

use surco::components::{Fields, Marked, Prose};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::text::{agreeing, counted};
use yunta_core::shown::PlanReview;
use yunta_core::Task;

use super::said;

/// The head of `review`: its title with how big it is; then its summary,
/// what keeps it from being proven when anything does, and what the person
/// asked for. Where the plan stands for its reader is the page's to say. A
/// count of its tests is not part of it: how a plan is proven is said once,
/// in its proof.
pub(super) fn head(review: &PlanReview, steps: &[Vec<&Task>]) -> Doc {
    let mut doc = Doc::new().with(Block::Title(
        Line::new()
            .push(Tone::Strong, "plan")
            .plain(format!(": {}", facts(review, steps.len()).join(", "))),
    ));
    if let Some(summary) = said(&review.plan.summary) {
        doc = doc.with(Prose(summary.to_string()));
    }
    let flaws = review.flaws().len();
    if flaws > 0 {
        doc = doc.with(Marked {
            mark: Mark::Caution,
            items: vec![format!(
                "{} {} this plan from being proven as it is written",
                counted(flaws, "problem"),
                agreeing(flaws, "keeps", "keep")
            )],
        });
    }
    if let Some(asked) = said(&review.asked) {
        doc = doc.with(Fields::new().push_if("you asked", asked));
    }
    doc
}

/// How big the plan is and what it holds for its reader, a fact each: its
/// tasks and the steps they take, the choices it made for them, what it
/// risks and what it leaves out.
fn facts(review: &PlanReview, steps: usize) -> Vec<String> {
    let plan = &review.plan;
    let tasks = counted(plan.tasks.len(), "task");
    let mut facts = vec![match steps {
        0 | 1 => tasks,
        steps => format!("{tasks} in {steps} steps"),
    }];
    let decided = super::change::decided_for_you(review).count();
    if decided > 0 {
        facts.push(format!("{decided} decided for you"));
    }
    if !plan.risks.is_empty() {
        facts.push(counted(plan.risks.len(), "risk"));
    }
    if !plan.out_of_scope.is_empty() {
        facts.push(format!("{} left out", plan.out_of_scope.len()));
    }
    facts
}
