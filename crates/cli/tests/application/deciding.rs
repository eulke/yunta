//! Deciding in the application: a run parked on a gate answered on its card, a round of
//! questions answered, and a plan read at its desk and decided there. Each answer is on
//! the run's log, and the run goes on to its end, as `resolve-gate` leaves it.

use yunta_testkit::workflows::{
    ASKS, EXECUTOR_ON_THE_MOCK, GATED, HANDS_ONE_QUESTION_OVER, PLANNER_HANDS_TWO_TASKS,
    PLANNER_ON_THE_MOCK, PLAN_THEN_GATE,
};
use yunta_testkit::{runs_root, wait_until, Checkout};

use super::{following, given_back, log, outcome, parked};

/// The log's line that records the decision on the run `id`, once the run finished and
/// its log says so, and how `yunta status` says it ended. The log is read once it holds
/// the run's end, since what the store holds reaches it later.
fn decided(checkout: &Checkout, id: &str) -> (String, String) {
    wait_until(
        || log(checkout, id).contains("\"run_finished\""),
        || format!("the run never finished:\n{}", log(checkout, id)),
    );
    let log = log(checkout, id);
    let decision = log
        .lines()
        .find(|line| line.contains("\"gate_resolved\""))
        .unwrap_or_else(|| panic!("no decision on the log:\n{log}"))
        .to_string();
    (decision, outcome(checkout, id))
}

#[test]
fn a_gate_answered_on_its_card_is_on_the_log_and_the_run_goes_on_to_its_end() {
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", GATED)
        .committed();
    let id = parked(&checkout, &["run", "wf.yaml"]);
    let mut terminal = following(&checkout);
    terminal.keys("a");
    terminal.wait_on_screen("▸ go", "the gate's card never stood");
    terminal.keys("\r");
    let (decision, ended) = decided(&checkout, &id);
    assert!(decision.contains("\"go\""), "{decision}");
    assert_eq!(ended, "finished");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

#[test]
fn a_round_answered_is_on_the_run_and_the_run_goes_on_to_its_end() {
    let checkout = Checkout::new()
        .config(EXECUTOR_ON_THE_MOCK)
        .workflow("wf", ASKS)
        .file("fixture.yaml", HANDS_ONE_QUESTION_OVER)
        .committed();
    let arguments = [
        "run",
        "wf.yaml",
        "--adapter",
        "mock",
        "--fixture",
        "fixture.yaml",
    ];
    let id = parked(&checkout, &arguments);
    let mut terminal = following(&checkout);
    terminal.keys("a");
    terminal.wait_on_screen("What changed?", "the round never stood");
    terminal.keys("the greeting names the person");
    terminal.keys("\r");
    wait_until(
        || log(&checkout, &id).contains("\"run_finished\""),
        || format!("the run never finished:\n{}", log(&checkout, &id)),
    );
    assert_eq!(outcome(&checkout, &id), "finished");
    let answers = runs_root(&checkout.home)
        .join(&id)
        .join("artifacts/ask/answers.yaml");
    let recorded = std::fs::read_to_string(answers).expect("the answers on the run");
    assert!(
        recorded.contains("the greeting names the person"),
        "{recorded}"
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// The card's `r` opens the plan's desk, where a number chosen and confirmed decides.
#[test]
fn a_plan_decided_at_its_desk_is_on_the_log_and_the_run_goes_on_to_its_end() {
    let checkout = Checkout::new()
        .config(PLANNER_ON_THE_MOCK)
        .workflow("wf", PLAN_THEN_GATE)
        .file("fixture.yaml", PLANNER_HANDS_TWO_TASKS)
        .committed();
    let arguments = [
        "run",
        "wf.yaml",
        "--adapter",
        "mock",
        "--fixture",
        "fixture.yaml",
    ];
    let id = parked(&checkout, &arguments);
    let mut terminal = following(&checkout);
    terminal.keys("a");
    terminal.wait_on_screen("▸ approve", "the plan's card never stood");
    terminal.keys("r");
    // At 80 columns the desk has no rail of its tasks: its keys say it stands.
    terminal.wait_on_screen("j k task", "the desk never opened");
    terminal.keys("1");
    terminal.wait_on_screen("enter confirms", "a number never asked to be confirmed");
    terminal.keys("\r");
    let (decision, ended) = decided(&checkout, &id);
    assert!(decision.contains("\"approve\""), "{decision}");
    assert_eq!(ended, "finished");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}
