//! What is spent against a cap, as its card rests on it: the cap, then what was spent, the
//! values two cells past the widest words of their column; a loop's ready tasks under them.

use surco::doc::Doc;
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::events::{
    BudgetCap, Claim, Escalation, Failure, FailureMenu, GateOption, GateWaitingPayload, Menu,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, NonEmpty, RunId};

use super::SpentAgainstItsCap;

/// `menu` asked with `claim`, continuing or pausing.
fn asked(claim: Claim, menu: Menu) -> GateWaitingPayload {
    let options = NonEmpty::from((
        GateOption::reserved(ReservedOption::Continue),
        vec![GateOption::reserved(ReservedOption::Abort)],
    ));
    Escalation::new(claim, menu, options).into_payload()
}

/// What `escalation` rests on, drawn plain, a line each.
fn drawn(escalation: &GateWaitingPayload) -> Vec<String> {
    let spent = SpentAgainstItsCap::asked_by(escalation).expect("a cap");
    let drawn = Terminal::on(Look::plain()).draw(&Doc::new().with_all(spent.blocks()));
    drawn.lines().map(str::to_string).collect()
}

#[test]
fn the_runs_token_cap_says_the_cap_then_the_tokens_spent_in_one_column() {
    let run = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    let cap = Menu::Budget(BudgetCap {
        cap: 100,
        spent: 200,
    });
    assert_eq!(
        drawn(&asked(Claim::TokensSpent { run }, cap)),
        [
            "  limits.max_tokens_per_run                             100",
            "  total input+output tokens derived from the event log  200",
        ]
    );
}

#[test]
fn a_loop_failed_at_its_cap_says_the_cap_what_it_ran_and_its_ready_tasks() {
    let node = NodeId::from_static("implement");
    let failure = Failure::LoopCapped {
        node: node.clone(),
        loop_cap: 2,
    };
    let menu = Menu::Failure(Box::new(FailureMenu {
        attempt: 1,
        failure,
        remedies: Vec::new(),
        stuck: None,
        reroute: None,
    }));
    assert_eq!(
        drawn(&asked(Claim::NodeFailed { node }, menu)),
        [
            "  limits.max_loop_iterations                2",
            "  iterations already run this invocation    2",
            "  the tasks document still has ready tasks",
        ]
    );
}

#[test]
fn a_menu_about_no_cap_rests_on_none() {
    let node = NodeId::from_static("lint");
    let failure = Failure::Message {
        outcome: "exit 1".into(),
    };
    let menu = Menu::Failure(Box::new(FailureMenu {
        attempt: 1,
        failure,
        remedies: Vec::new(),
        stuck: None,
        reroute: None,
    }));
    let escalation = asked(Claim::NodeFailed { node }, menu);
    assert!(SpentAgainstItsCap::asked_by(&escalation).is_none());
}
