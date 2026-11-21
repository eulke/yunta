//! A run's step opens the stage that shows what its command shows, through the one mapping
//! every stage and every opening reads.

use yunta_core::step::{Choice, NextStep};
use yunta_core::RunId;

use super::{RunStage, Stage, StatsStage};
use crate::prompt::Opens;

/// The run a stage is about.
fn this_run() -> RunId {
    "01M7THISRUN000000000000000".parse().expect("a run's id")
}

/// `review` opens the run's screen on its plan, `resolve-gate` and `status` on its card,
/// `stats` the run's numbers.
#[test]
fn a_run_s_step_opens_the_stage_that_shows_it() {
    let opened = |step: NextStep| format!("{:?}", Stage::opened_by(&step));
    let run_opened_on = |opens| {
        let stage = Stage::Run(RunStage::opened_on(this_run(), opens));
        format!("{:?}", Some(stage))
    };
    let review = NextStep::Review { run: this_run() };
    assert_eq!(opened(review), run_opened_on(Opens::ThePlan));
    let resolve_gate = NextStep::ResolveGate {
        run: this_run(),
        choice: Choice::Asked,
    };
    assert_eq!(opened(resolve_gate), run_opened_on(Opens::TheCard));
    assert_eq!(
        opened(NextStep::status(&this_run())),
        run_opened_on(Opens::TheCard)
    );
    let stats = NextStep::Stats { run: this_run() };
    let its_numbers = Stage::Stats(StatsStage::of_run(this_run()));
    assert_eq!(opened(stats), format!("{:?}", Some(its_numbers)));
}
