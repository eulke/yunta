//! How the surfaces read a run parked on a departure: one aborted on a
//! terminal, and one owed where no terminal was there to ask.

use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

use super::{answering, common, fixture, WORKFLOW};

/// An aborted departure leaves no menu to choose from, its question comes
/// back on resume; every surface that asks whether a parked run waits on a
/// menu answers what `status` answers, so none offers a choice there is not.
#[tokio::test]
async fn an_aborted_departure_is_read_the_same_by_every_surface() {
    let bench = Bench::new();
    let RunReport { state, .. } = bench
        .run_with_interaction(WORKFLOW, &fixture(""), &answering("abort", None))
        .await;
    let manifest = bench.manifest();
    let menu = yunta_engine::current_escalation(&manifest, &bench.run_dir(), &state)
        .await
        .expect("the parked run's menu is read");
    assert_eq!(
        yunta_engine::awaits_decision(&manifest, &state),
        menu.map(|(node, _)| node)
    );
}

/// A departure a run owes with no terminal to ask it on is answered from the
/// menu the run parks with, the way `yunta resolve-gate` answers it, and the
/// next wake settles it with no one there.
#[tokio::test]
async fn a_departure_owed_without_a_terminal_is_answered_from_the_parked_menu() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture("")).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    common::answer_parked(&bench, "accept")
        .await
        .expect("the parked menu offers the departure's answers");
    let RunReport { terminal, .. } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
}
