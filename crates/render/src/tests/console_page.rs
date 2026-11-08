//! The gate's page at a terminal, as the person answering it reads it.

use surco::doc::{Block, Doc};
use surco::golden::{assert_golden, Environment, ENVIRONMENTS};
use surco::look::Look;
use surco::paint::Paint;
use surco::surface::{Surface, Terminal};
use surco::width::cell_width;
use yunta_core::events::{
    AuthorGate, Claim, Escalation, GateOption, GateWaitingPayload, Leads, Menu, WhyWithheld,
    Withheld,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::shown::PlanReview;
use yunta_core::{NodeId, NonEmpty, OptionId};

use super::documents::{review, run, shown_plan};
use super::publication::gate_goldens;
use crate::page::{Gate, Page};

/// The payload of the escalation a gate of the greet plan raises: its claim
/// asks to approve the plan's change, its menu offers approve, adjust and
/// abort, and approve is withheld for the plan's flaws.
pub(super) fn greet_escalation(review: &PlanReview) -> GateWaitingPayload {
    let summary = review
        .plan
        .summary
        .clone()
        .expect("the greet plan names its change");
    let flaws = NonEmpty::new(review.flaws()).expect("the greet plan has flaws");
    let approve = OptionId::from_static("approve");
    Escalation::new(
        Claim::ApprovePlan { summary },
        Menu::Gate(AuthorGate {
            assignee: "lead".into(),
            node: Some(NodeId::from("approve-plan")),
        }),
        NonEmpty::from((
            GateOption::leading(
                approve.clone(),
                Leads::On {
                    next: vec![NodeId::from("implement")],
                    settles: false,
                },
            ),
            vec![
                GateOption::leading(
                    OptionId::from_static("adjust"),
                    Leads::Back {
                        node: NodeId::from("plan"),
                        resumes: true,
                    },
                ),
                GateOption::reserved(ReservedOption::Abort),
            ],
        )),
    )
    .withholding(vec![Withheld {
        option: approve,
        because: WhyWithheld::Unprovable { flaws },
    }])
    .into_payload()
}

/// The look `environment` draws a line with, unpainted: its glyphs and its
/// width, without the color a golden keeps.
pub(super) fn unpainted(environment: &Environment) -> Look {
    Look {
        paint: Paint::Plain,
        ..Look::of(environment)
    }
}

/// Each line of `page`, drawn unpainted at the width `environment` has, fits in
/// that width.
pub(super) fn assert_fits(page: &Doc, environment: &Environment) {
    for line in Terminal::on(unpainted(environment)).draw(page).lines() {
        let cells = cell_width(line);
        assert!(cells <= environment.width, "{cells} cells in `{line}`");
    }
}

/// Whether `block` is the section that says what weighs on a decision.
fn weighing(block: &Block) -> bool {
    matches!(block, Block::Section(before) if before.title.text() == "before you decide")
}

/// The page opens on its headline and ends on what weighs on the decision: its
/// last block is the section that says it, and no command that resolves a gate
/// is drawn.
fn assert_headline_and_weighing(page: &Doc, plain: &str) {
    assert!(
        matches!(page.blocks().first(), Some(Block::Headline(_))),
        "{plain}"
    );
    let headline = plain.lines().next().unwrap_or_default();
    assert!(
        headline.contains("decision") && headline.contains("needs you"),
        "{plain}"
    );
    assert!(!plain.contains("resolve-gate"), "{plain}");
    assert!(
        page.blocks().last().is_some_and(weighing),
        "the page does not end on what weighs on the decision:\n{plain}"
    );
}

/// The page a terminal with no room for a plan's desk prints above its card: the
/// plan the decision is about, and what weighs on it.
fn console_page() -> Doc {
    let review = review();
    let shown = vec![shown_plan(review.clone())];
    let escalation = greet_escalation(&review);
    let run = run();
    Gate::InPlaceOfTheDesk {
        shown: Some((&run, shown.as_slice())),
        escalation: &escalation,
    }
    .document(&Look::plain())
}

/// A gate's page at a terminal matches its goldens in every environment, fits
/// each width, opens on its headline and ends on what weighs on the decision.
/// Every structural check runs in every environment before the first golden is
/// compared, so a page that is wrong says how in each environment, goldens or
/// not.
#[test]
fn a_gate_s_page_at_a_terminal_matches_its_goldens() {
    for environment in &ENVIRONMENTS {
        let page = console_page();
        assert_fits(&page, environment);
        let plain = Terminal::on(unpainted(environment)).draw(&page);
        assert_headline_and_weighing(&page, &plain);
    }
    for environment in &ENVIRONMENTS {
        let page = console_page();
        let drawn = Terminal::on(Look::of(environment)).draw(&page);
        assert_golden(&environment.golden(&gate_goldens(), "console"), &drawn);
    }
}
