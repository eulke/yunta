//! The small runs parked on what a person answers.

use yunta_api::{FollowNews, Followed, RunNews, Stop};
use yunta_core::events::{EventPayload, Failure, GateEvent, Menu, PauseReason, TaskCause};
use yunta_core::reserved::ReservedOption;
use yunta_core::NodeId;
use yunta_testkit_ui::small_runs::asking;

use crate::{at, built_twice, stand, waiting};

#[tokio::test]
async fn a_console_gate_parks_on_the_consoles_menu() {
    let run = built_twice(asking::console_gate).await;
    let stand = stand(&run).await;
    assert!(waiting(&stand.frame.phase), "{:?}", stand.frame.phase);
    let Some(PauseReason::Escalation(escalation)) = &stand.pause else {
        panic!("the gate parks on an escalation: {:?}", stand.pause);
    };
    assert!(
        matches!(escalation.menu(), Some(Menu::Console(_))),
        "{escalation:?}"
    );
}

/// The gate's own node, and the document it shows: the release notes the node before it
/// wrote.
const SHOWS: (&str, &str) = ("publish", "release-notes.md");

#[tokio::test]
async fn a_gate_that_shows_the_release_notes_parks_on_its_own_menu() {
    let run = built_twice(asking::notes_gate).await;
    let stand = stand(&run).await;
    let Some(PauseReason::Escalation(escalation)) = &stand.pause else {
        panic!("the gate parks on an escalation: {:?}", stand.pause);
    };
    assert!(
        matches!(escalation.menu(), Some(Menu::Gate(gate)) if gate.node.as_ref().is_some_and(|node| node.as_str() == SHOWS.0)),
        "{escalation:?}"
    );
    let shown: Vec<String> = escalation
        .shows()
        .iter()
        .map(|shown| format!("{:?}", shown.artifact))
        .collect();
    assert!(
        shown.len() == 1 && shown[0].contains(SHOWS.1),
        "the gate shows {shown:?}"
    );
}

/// A departure no one answered at the terminal parks as the task's stop, owing the
/// departure: the menu the engine rebuilds is the failure's, whose remedies accept the
/// departure or send it back. `Menu::Departure` is the question a live terminal is put.
#[tokio::test]
async fn a_departure_parks_owing_its_decision() {
    let run = built_twice(asking::departure).await;
    let stand = stand(&run).await;
    assert!(
        matches!(&stand.pause, Some(PauseReason::NodeFailed { node, failure: Failure::TasksStopped { stopped } })
            if node.as_str() == "implement"
                && matches!(stopped[0].cause, TaskCause::DeviationOwed { .. })),
        "{:?}",
        stand.pause
    );
    let Some(Menu::Failure(menu)) = &stand.menu else {
        panic!("the decision is the stopped task's: {:?}", stand.menu);
    };
    let offered: Vec<_> = menu.remedies.iter().map(|remedy| remedy.option).collect();
    assert!(
        offered.contains(&ReservedOption::Accept) && offered.contains(&ReservedOption::SendBack),
        "{offered:?}"
    );
}

#[tokio::test]
async fn a_spent_token_cap_parks_on_the_caps_menu() {
    let run = built_twice(asking::token_cap).await;
    let stand = stand(&run).await;
    assert_eq!(
        stand.pause,
        Some(PauseReason::BudgetExhausted {
            spent: 200,
            cap: 100
        })
    );
    assert!(
        matches!(stand.menu, Some(Menu::Budget(_))),
        "{:?}",
        stand.menu
    );
}

/// A loop that wants an iteration past its cap, with no one at the terminal to lift it,
/// ends at the cap and parks: the menu the engine rebuilds is the capped loop's
/// failure. `Menu::LoopCap` is the question a live terminal is put.
#[tokio::test]
async fn a_spent_loop_cap_parks_on_the_capped_loop() {
    let run = built_twice(asking::loop_cap).await;
    let stand = stand(&run).await;
    let capped = Failure::LoopCapped {
        node: NodeId::from("implement"),
        loop_cap: 2,
    };
    assert_eq!(
        stand.pause,
        Some(PauseReason::NodeFailed {
            node: NodeId::from("implement"),
            failure: capped.clone(),
        })
    );
    let Some(Menu::Failure(menu)) = &stand.menu else {
        panic!("the decision is the capped loop's: {:?}", stand.menu);
    };
    assert_eq!(menu.failure, capped);
}

#[tokio::test]
async fn a_failure_parks_on_its_menu() {
    let run = built_twice(asking::failure).await;
    let stand = stand(&run).await;
    assert!(waiting(&stand.frame.phase), "{:?}", stand.frame.phase);
    assert!(
        matches!(stand.menu, Some(Menu::Failure(_))),
        "{:?}",
        stand.menu
    );
}

#[tokio::test]
async fn a_plan_sent_back_works_again_and_is_approved_on_its_second_ask() {
    let run = built_twice(asking::sent_back).await;
    let events = run.events(run.run());
    let answers: Vec<String> = events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Resolved(resolved))) => {
                Some(format!("{resolved:?}"))
            }
            _ => None,
        })
        .collect();
    assert_eq!(answers.len(), 2, "{answers:?}");
    assert!(
        answers[0].contains("adjust") && answers[1].contains("approve"),
        "{answers:?}"
    );
    let adjusted = at(&events, "gate_resolved", Some("approve-plan"), 0);
    let again = at(&events, "node_started", Some("plan"), adjusted);
    let working = at(&events, "agent_session_opened", Some("plan"), again);
    at(&events, "node_finished", Some("plan"), working);
    assert!(matches!(
        stand(&run).await.frame.phase,
        yunta_replay::frame::RunPhase::Finished
    ));
}

#[tokio::test]
async fn a_round_whose_answers_were_refused_is_answered_after() {
    let run = built_twice(asking::refused).await;
    let events = run.events(run.run());
    let refused = events
        .iter()
        .position(|event| {
            matches!(event.payload(), Some(EventPayload::Run(yunta_core::events::RunEvent::Paused(paused)))
                if matches!(paused.pause, PauseReason::AnswersRefused { .. }))
        })
        .expect("the answers left the required question out, and were refused");
    at(&events, "questions_answered", Some("ask"), refused);
    assert!(matches!(
        stand(&run).await.frame.phase,
        yunta_replay::frame::RunPhase::Finished
    ));
}

#[tokio::test]
async fn a_decision_whose_shown_document_fails_its_shape_reads_as_unread() {
    let run = built_twice(asking::unread).await;
    let workspace = run.workspace();
    let project = workspace.project().expect("the small run's repository");
    let followed = Followed {
        field: None,
        run: Some(run.run().clone()),
    };
    let mut following = project.follow(followed).await.expect("the run, followed");
    let stop = Stop::new();
    let mut came = Vec::new();
    loop {
        let news = tokio::time::timeout(std::time::Duration::from_secs(10), following.next(&stop))
            .await
            .unwrap_or_else(|_| panic!("what the run waits on is never read: {came:?}"))
            .expect("the run, followed");
        match news {
            FollowNews::Run {
                news: RunNews::AwaitedUnread { why, .. },
                ..
            } => {
                assert!(why.contains("spec"), "{why}");
                return;
            }
            other => came.push(format!("{other:?}")),
        }
    }
}
