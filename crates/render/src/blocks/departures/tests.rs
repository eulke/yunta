//! A departure from the plan, as its card rests on it: what the plan decided, what the
//! session says instead and why, each quoted under its words; short of rows, why gives way.

use surco::doc::Doc;
use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::events::{
    Claim, DepartsFrom, DeviationDeclaredPayload, Escalation, Failure, FailureMenu, GateOption,
    GateWaitingPayload, Menu, TaskCause, TaskStop,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, NonEmpty, TaskId};

use super::Departures;

/// A loop that failed with its one task stopped on a departure owed a person.
fn departing() -> GateWaitingPayload {
    let departure = DeviationDeclaredPayload {
        task_id: TaskId::from("greet"),
        from: DepartsFrom::Shape("Greeting".to_string()),
        planned: "Hello, and good night".to_string(),
        instead: "Hello".to_string(),
        why: "the second half needs the clock".to_string(),
    };
    let failure = Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: TaskId::from("greet"),
            cause: TaskCause::DeviationOwed {
                deviations: NonEmpty::from((departure, Vec::new())),
            },
        }],
    };
    let menu = FailureMenu {
        attempt: 1,
        failure,
        remedies: Vec::new(),
        stuck: None,
        reroute: None,
    };
    let options = NonEmpty::from((GateOption::reserved(ReservedOption::Accept), Vec::new()));
    let node = NodeId::from_static("implement");
    Escalation::new(
        Claim::NodeFailed { node },
        Menu::Failure(Box::new(menu)),
        options,
    )
    .into_payload()
}

/// `blocks` drawn plain, a line each.
fn drawn(blocks: Vec<surco::doc::Block>) -> Vec<String> {
    let drawn = Terminal::on(Look::plain()).draw(&Doc::new().with_all(blocks));
    drawn.lines().map(str::to_string).collect()
}

#[test]
fn a_departure_says_what_the_plan_decided_what_the_session_says_instead_and_why() {
    let escalation = departing();
    let departures = Departures::asked_by(&escalation);
    assert_eq!(
        drawn(departures.blocks(Glyphs::Unicode)),
        [
            "the plan decided",
            "│ Hello, and good night",
            "",
            "the session says instead",
            "│ Hello",
            "",
            "because",
            "│ the second half needs the clock",
        ]
    );
    let abridged = departures.abridged(Glyphs::Unicode);
    let [shorter] = <[_; 1]>::try_from(abridged).ok().expect("one shorter form");
    assert_eq!(
        drawn(shorter),
        [
            "the plan decided",
            "│ Hello, and good night",
            "",
            "the session says instead",
            "│ Hello",
        ],
        "why gives way first"
    );
}
