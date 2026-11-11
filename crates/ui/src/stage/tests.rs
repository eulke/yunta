//! A stage takes what its own work came to, and leaves what another run's came to,
//! which reaches it only because it stands where the person is when the answer comes.

use yunta_core::RunId;

use super::{DecisionStage, RunStage, Want};
use crate::prompt::Opens;
use crate::Outcome;

/// The run a stage is about.
fn this_run() -> RunId {
    "01M7THISRUN000000000000000".parse().expect("a run's id")
}

/// Another run, whose work the person asked for on a stage they left.
fn another_run() -> RunId {
    "01M7ANOTHERRUN000000000000".parse().expect("a run's id")
}

#[test]
fn a_decision_goes_back_only_once_its_own_run_is_decided() {
    let mut stage = DecisionStage::of(this_run(), Opens::TheCard);
    let elsewhere = stage.answered(Outcome::Decided { run: another_run() });
    assert!(
        elsewhere.is_empty(),
        "another run's decision leaves it standing"
    );
    let own = stage.answered(Outcome::Decided { run: this_run() });
    assert!(matches!(own.as_slice(), [Want::Back]));
}

#[test]
fn a_run_follows_again_only_once_its_own_run_goes_on() {
    let mut stage = RunStage::of(this_run());
    let elsewhere = stage.answered(Outcome::Resumed { run: another_run() });
    assert!(
        elsewhere.is_empty(),
        "another run resumed asks nothing here"
    );
    let own = stage.answered(Outcome::Resumed { run: this_run() });
    assert_eq!(own.len(), 2, "its following and its page, read again");
}
