//! A reading asked again supersedes the reading of its kind its asker waits on, and never an
//! act of its kind, nor another stage's: checking doctor again leaves its sessions running, a
//! dry run leaves a collection collecting, and a page under another keeps its reading. Work
//! another stage asks while it is under way is not asked twice: it joins the askers.

use yunta_core::RunId;

use super::routed::Asker;
use super::State;
use crate::{Operation, Request, Stage};

/// A state as it opens, and two stages, each someone of its own.
fn two_askers() -> (State, Asker, Asker) {
    let (mut state, _) =
        State::opened(chrono::DateTime::from_timestamp(1_800_000_000, 0).expect("an instant"));
    let one = Asker::Stage(state.stood(Stage::Signpost).id);
    let other = Asker::Stage(state.stood(Stage::Signpost).id);
    (state, one, other)
}

/// What asking `then` by one stage, with `first` under way, asked by `first_by` it or another,
/// lets go.
fn let_go_by(first: Operation, then: Operation, first_by_another: bool) -> Vec<Request> {
    let (mut state, one, other) = two_askers();
    let first_by = match first_by_another {
        true => other,
        false => one,
    };
    state.asked(first, first_by);
    state
        .asked(then, one)
        .into_iter()
        .filter(|request| matches!(request, Request::LetGo(_)))
        .collect()
}

#[test]
fn a_reading_lets_go_the_reading_of_its_kind_its_asker_waits_on() {
    let pull_request = |run: &'static str| Operation::ReadPullRequest {
        run: RunId::from_static(run),
    };
    let promoted = pull_request("01M7PROMOTED00000000000000");
    let successor = pull_request("01M7SUCCESSOR0000000000000");
    let let_go = let_go_by(promoted.clone(), successor.clone(), false);
    assert_eq!(
        let_go.len(),
        1,
        "a successor's pull request read supersedes its run's under way"
    );
    let let_go = let_go_by(promoted, successor, true);
    assert!(
        let_go.is_empty(),
        "another stage's reading of the kind stays its own: {let_go:?}"
    );
    let let_go = let_go_by(
        Operation::Collect { dry_run: true },
        Operation::ReadCatalog,
        false,
    );
    assert!(
        let_go.is_empty(),
        "a reading of another kind lets nothing go"
    );
}

#[test]
fn a_reading_never_lets_go_an_act_of_its_kind_under_way() {
    let sessions = let_go_by(
        Operation::Doctor { sessions: true },
        Operation::Doctor { sessions: false },
        false,
    );
    assert!(sessions.is_empty(), "{sessions:?}");
    let collecting = let_go_by(
        Operation::Collect { dry_run: false },
        Operation::Collect { dry_run: true },
        false,
    );
    assert!(collecting.is_empty(), "{collecting:?}");
}

#[test]
fn work_another_stage_asks_while_it_is_under_way_is_asked_once_and_let_go_last() {
    let (mut state, one, other) = two_askers();
    let asked = state.asked(Operation::ReadCatalog, one);
    let [Request::Perform(ticket, _)] = asked.as_slice() else {
        panic!("asked once: {asked:?}");
    };
    let ticket = *ticket;
    assert!(state.asked(Operation::ReadCatalog, other).is_empty());
    let reads = |waited: &Operation| *waited == Operation::ReadCatalog;
    assert!(
        state.let_go(one, reads).is_empty(),
        "the other still waits on it"
    );
    assert_eq!(state.let_go(other, reads), [Request::LetGo(ticket)]);
    let waited = state.waiting_on();
    assert!(!waited.iter().any(|(on, _)| *on == ticket), "{waited:?}");
}
