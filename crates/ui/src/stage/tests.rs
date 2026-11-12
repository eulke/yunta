//! A stage takes what its own work came to, and leaves what another run's came to,
//! which reaches it only because it stands where the person is when the answer comes.

use yunta_core::step::NextStep;
use yunta_core::RunId;

use super::{DecisionStage, RunStage, Stage, StatsStage, Want};
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

/// A run's step opens the stage that shows what its command shows, through the one
/// mapping every stage and every opening reads: `review` the run's decision open on its
/// plan, `stats` the run's numbers.
#[test]
fn a_run_s_step_opens_the_stage_that_shows_it() {
    let review = Stage::opened_by(&NextStep::Review { run: this_run() });
    let on_its_plan = Stage::Decision(DecisionStage::of(this_run(), Opens::ThePlan));
    assert_eq!(format!("{review:?}"), format!("{:?}", Some(on_its_plan)));
    let stats = Stage::opened_by(&NextStep::Stats { run: this_run() });
    let its_numbers = Stage::Stats(StatsStage::of_run(this_run()));
    assert_eq!(format!("{stats:?}"), format!("{:?}", Some(its_numbers)));
}
