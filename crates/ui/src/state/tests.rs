//! A reading asked again supersedes the readings of its kind under way, and never an act
//! of its kind: checking doctor again leaves its sessions running, and a dry run leaves
//! a collection collecting.

use yunta_core::RunId;

use super::State;
use crate::{Operation, Request};

/// What asking `then`, with `first` under way, lets go.
fn let_go_by(first: Operation, then: Operation) -> Vec<Request> {
    let (mut state, _) =
        State::opened(chrono::DateTime::from_timestamp(1_800_000_000, 0).expect("an instant"));
    state.asked(first);
    state
        .asked(then)
        .into_iter()
        .filter(|request| matches!(request, Request::LetGo(_)))
        .collect()
}

#[test]
fn a_reading_lets_go_the_reading_of_its_kind_under_way() {
    let page = |run: &'static str| Operation::ReadPage {
        run: RunId::from_static(run),
    };
    let let_go = let_go_by(
        page("01M7PROMOTED00000000000000"),
        page("01M7SUCCESSOR0000000000000"),
    );
    assert_eq!(
        let_go.len(),
        1,
        "a successor's page read supersedes its run's under way"
    );
    let let_go = let_go_by(Operation::Collect { dry_run: true }, Operation::ReadCatalog);
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
    );
    assert!(sessions.is_empty(), "{sessions:?}");
    let collecting = let_go_by(
        Operation::Collect { dry_run: false },
        Operation::Collect { dry_run: true },
    );
    assert!(collecting.is_empty(), "{collecting:?}");
}
