use surco::words::Standing;
use yunta_core::events::{Failure, PauseReason};
use yunta_core::{NodeId, QuestionId, RunId};

use super::{asked_questions, sentence, standing};

#[test]
fn a_node_that_waits_for_a_reset_names_the_moment_it_resumes() {
    let waiting = PauseReason::AwaitingReset {
        node: NodeId::from("review"),
        at: yunta_testkit_core::fixed_now(),
    };
    assert_eq!(
        sentence(&waiting),
        "node `review` waits for its limit to reset at 2026-01-01 00:00 UTC, and the run \
         resumes on its own then"
    );
}

#[test]
fn a_failed_node_pauses_on_the_headline_of_its_failure() {
    let failed = PauseReason::NodeFailed {
        node: NodeId::from("lint"),
        failure: Failure::Message {
            outcome: "exit 1".into(),
        },
    };
    assert_eq!(sentence(&failed), "node `lint` failed: exit 1");
}

/// The pause an older log recorded when a gate's pull request closed reads
/// as the headline of that failure, the pull request it names left out.
#[test]
fn a_closed_pull_request_pauses_on_its_headline() {
    let closed = PauseReason::NodeFailed {
        node: NodeId::from("approve"),
        failure: Failure::PullRequestClosed {
            closed_pull_request: Some("https://mock.forge/pr/1".to_string()),
        },
    };
    assert_eq!(
        sentence(&closed),
        "node `approve` failed: the pull request was closed without approval"
    );
}

#[test]
fn a_pause_a_person_chose_reads_as_the_choice() {
    assert_eq!(sentence(&PauseReason::Cancelled), "interrupted by a person");
}

/// A person's pause at a gate says the decision is still open, and adds any note.
#[test]
fn a_person_paused_the_run_with_its_decision_open() {
    let paused = PauseReason::GateAborted {
        node: NodeId::from("approve-plan"),
        free_text: None,
    };
    assert_eq!(
        sentence(&paused),
        "a person paused the run at node `approve-plan`, its decision still open"
    );
    let with_note = PauseReason::GateAborted {
        node: NodeId::from("approve-plan"),
        free_text: Some("not this quarter".to_string()),
    };
    assert_eq!(
        sentence(&with_note),
        "a person paused the run at node `approve-plan`, its decision still open: \
         not this quarter"
    );
}

/// A closed pull request names the node, leaves its decision open, and gives the URL.
#[test]
fn a_closed_pull_request_reads_as_a_pause_with_its_decision_open() {
    let closed = PauseReason::PullRequestClosed {
        node: NodeId::from("approve"),
        url: "https://mock.forge/pr/1".to_string(),
    };
    assert_eq!(
        sentence(&closed),
        "node `approve`'s pull request was closed, its decision still open \
         (https://mock.forge/pr/1)"
    );
}

/// A person's gate pause and a closed pull request both have the stopped standing.
#[test]
fn both_pauses_read_as_stopped() {
    let gate = PauseReason::GateAborted {
        node: NodeId::from("approve"),
        free_text: None,
    };
    let closed = PauseReason::PullRequestClosed {
        node: NodeId::from("approve"),
        url: "https://mock.forge/pr/1".to_string(),
    };
    assert_eq!(standing(&gate), Standing::Stopped);
    assert_eq!(standing(&closed), Standing::Stopped);
}

#[test]
fn a_loop_that_overran_names_its_limit() {
    let overrun = PauseReason::LoopOverrun {
        node: NodeId::from("fix-lint"),
        cap: 3,
    };
    assert_eq!(
        sentence(&overrun),
        "loop `fix-lint` exceeded `limits.max_loop_iterations` (3)"
    );
}

/// What goes on from here is the decision under it, on every interface, so the sentence
/// says what happened and nothing about who can answer.
#[test]
fn a_run_on_its_token_cap_says_what_it_spent_and_its_limit() {
    assert_eq!(
        sentence(&PauseReason::BudgetExhausted {
            spent: 100,
            cap: 100
        }),
        "its token budget is spent (100 tokens with `limits.max_tokens_per_run: 100`)"
    );
}

#[test]
fn a_blocked_run_names_what_its_node_waits_on() {
    let blocked = PauseReason::Blocked {
        node: NodeId::from("lint"),
        on: vec![NodeId::from("build"), NodeId::from("fix-lint")],
    };
    assert_eq!(
        sentence(&blocked),
        "no node is runnable: `lint` waits on `build`, `fix-lint`, which this run left \
         unresolved"
    );
}

#[test]
fn a_parked_child_names_its_run_and_carries_its_own_line() {
    let parked = PauseReason::ChildPaused {
        node: NodeId::from("grill"),
        run: RunId::from("child"),
        child: Box::new(PauseReason::Cancelled),
    };
    assert_eq!(
        sentence(&parked),
        "node `grill`'s child run is parked: child run `child` paused, and resuming this \
         run resumes it:\n  interrupted by a person"
    );
}

/// The one sentence every surface says a node's unanswered questions
/// with — the node's own label, the run's pause, the chronicle.
#[test]
fn questions_awaiting_an_answer_are_said_one_way() {
    assert_eq!(
        asked_questions(&[QuestionId::from("q-scope")]),
        "asked 1 question: `q-scope`"
    );
    assert_eq!(
        asked_questions(&[QuestionId::from("q-scope"), QuestionId::from("q-api")]),
        "asked 2 questions: `q-scope`, `q-api`"
    );
}
