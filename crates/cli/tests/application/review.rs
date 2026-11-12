//! `yunta review <run>` at a terminal: the application opened on the plan the run waits on,
//! over the run and the field, as a person walks there; an option chosen there is on the
//! run's log, and the application goes on with the run. Off a terminal, and refused, it is
//! `plan_desk.rs`'s.

use yunta_testkit::workflows::{PLANNER_HANDS_TWO_TASKS, PLANNER_ON_THE_MOCK, PLAN_THEN_GATE};
use yunta_testkit::{stdout, wait_until, yunta_at, yunta_on_terminal, Checkout, Terminal};

use super::{given_back, log, outcome, parked, FOLLOWED, THE_FIELD};

/// What the plan's desk says of `esc` where `review` opens it: the person goes back to the
/// run, which still waits.
const AT_THE_PLAN: &str = "esc leaves the run waiting";

/// A repository whose run parks on its plan's gate, and that run's whole id.
fn parked_on_its_plan() -> (Checkout, String) {
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
    (checkout, id)
}

/// `yunta review` of the run `id` at a terminal, once the plan stands on the other screen.
fn reviewing(checkout: &Checkout, id: &str) -> Terminal {
    let terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &["review", id]);
    terminal.wait_on_screen(AT_THE_PLAN, "review never opened on the plan");
    assert!(terminal.screen().alternate, "on the other screen");
    terminal
}

#[test]
fn review_opens_on_the_plan_over_the_run_and_the_field() {
    let (checkout, id) = parked_on_its_plan();
    let mut terminal = reviewing(&checkout, &id);
    assert!(terminal.screen().holds("Plan registered. Approve?"));
    terminal.keys("\u{1b}");
    terminal.wait_on_screen(FOLLOWED, "esc never reached the run");
    terminal.keys("\u{1b}");
    terminal.wait_on_screen(THE_FIELD, "esc again never reached the field");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
    assert_eq!(outcome(&checkout, &id), "needs you", "nothing was decided");
}

/// A number chosen at the plan and confirmed decides, the application goes on with the
/// run, and the run goes on to its end.
#[test]
fn an_option_chosen_in_review_is_on_the_log_and_the_application_goes_on_with_the_run() {
    let (checkout, id) = parked_on_its_plan();
    let mut terminal = reviewing(&checkout, &id);
    terminal.keys("1");
    terminal.wait_on_screen("enter confirms", "a number never asked to be confirmed");
    terminal.keys("\r");
    terminal.wait_on_screen(FOLLOWED, "the application never went on with the run");
    wait_until(
        || log(&checkout, &id).contains("\"run_finished\""),
        || format!("the run never finished:\n{}", log(&checkout, &id)),
    );
    let decided = log(&checkout, &id);
    let decision = decided
        .lines()
        .find(|line| line.contains("\"gate_resolved\""))
        .unwrap_or_else(|| panic!("no decision on the log:\n{decided}"));
    assert!(decision.contains("\"approve\""), "{decision}");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

#[test]
fn ctrl_c_in_review_exits_130_with_the_run_still_waiting() {
    let (checkout, id) = parked_on_its_plan();
    let mut terminal = reviewing(&checkout, &id);
    terminal.keys("\u{3}");
    assert_eq!(given_back(&mut terminal), Some(130));
    let status = yunta_at!(&checkout, &["status", &id]);
    assert!(stdout(&status).contains("needs you"), "{}", stdout(&status));
}
