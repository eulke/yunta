//! The field's cut as the design draws its field: the design's run asking at its own
//! terminal at `tests`, the one run that asks there; the runs that need the person and the
//! one stalled the design lists first; and a run in sight whose mode leaves a node out.

use chrono::{DateTime, Utc};
use yunta_api::{FollowedField, FollowedRun};
use yunta_core::RunId;
use yunta_replay::frame::{NodeStanding, RunPhase};
use yunta_testkit_ui::cut::{Cut, Pointed, TARGET_CUTS};

use crate::design;
use crate::screens::field_here;

/// The node the design's run asks about at its own terminal in the design's field.
const ASKED_AT: &str = "tests";

/// The runs the design's field lists first of those that need the person and of those
/// stalled, by their handles.
const THE_DESIGNS_OWN: [&str; 2] = ["EZRPE9", "EKPBGE"];

/// Each run of `field` whose engine asks a person at its terminal at `now`, with the node
/// it asks about.
fn asking_at_their_terminal(field: &FollowedField, now: DateTime<Utc>) -> Vec<(RunId, String)> {
    field
        .runs
        .iter()
        .filter_map(|run| {
            let asking = run.frame(now).asking?;
            let node = asking.prompt.node.map(|node| node.to_string());
            Some((run.run.clone(), node.unwrap_or_default()))
        })
        .collect()
}

/// Whether `run` stands in sight at `now`, not closed, with a node its mode leaves out.
fn leaves_a_node_out_in_sight(run: &FollowedRun, now: DateTime<Utc>) -> bool {
    let frame = run.frame(now);
    let closed = matches!(
        frame.phase,
        RunPhase::Finished
            | RunPhase::Failed { .. }
            | RunPhase::Cancelled
            | RunPhase::Promoted { .. }
            | RunPhase::Broken { .. }
    );
    !closed
        && frame
            .nodes
            .iter()
            .any(|node| matches!(node.state, NodeStanding::Skipped))
}

/// The two cuts that press `r` in the field, each with the run pointed at and whether that
/// run waits on a decision the engine rebuilds: the note `r` leaves on a run a decision
/// holds, and the run `r` resumes, whose pause no option answers.
const PRESSING_R: [(&str, &str, bool); 2] = [
    ("field-note", "EZRPE9", true),
    ("field-resumed", "C0NS0E", false),
];

#[tokio::test]
async fn r_is_pressed_on_a_run_a_decision_holds_and_on_one_it_resumes() {
    let field = TARGET_CUTS
        .iter()
        .find(|target| target.name == "field")
        .expect("the field's own target");
    let cut = Cut::of_target("field").await;
    let read = field_here(&cut).await;
    for (name, handle, decided) in PRESSING_R {
        let target = TARGET_CUTS
            .iter()
            .find(|target| target.name == name)
            .unwrap_or_else(|| panic!("no target is named `{name}`"));
        assert_eq!(target.stands, field.stands, "`{name}` stands on the field");
        assert_eq!(target.pointed, Some(Pointed::Run(handle)), "`{name}`");
        assert_eq!(target.keys, ["r"], "`{name}`");
        let pointed = read
            .runs
            .iter()
            .find(|run| run.run.handle() == handle)
            .unwrap_or_else(|| panic!("`{handle}` stands in no row"));
        assert_eq!(
            pointed.menu, decided,
            "`{name}`: whether `{handle}` waits on a decision"
        );
    }
}

#[tokio::test]
async fn the_field_holds_the_designs_run_asking_at_its_terminal_and_a_node_left_out() {
    let cut = Cut::of_target("field").await;
    let field = field_here(&cut).await;
    assert_eq!(
        asking_at_their_terminal(&field, cut.now()),
        [(design(), ASKED_AT.to_string())],
        "the runs asking at their own terminal"
    );
    let handles: Vec<String> = field
        .runs
        .iter()
        .map(|run| run.run.handle().to_string())
        .collect();
    for handle in THE_DESIGNS_OWN {
        assert!(
            handles.iter().any(|each| each == handle),
            "`{handle}` stands in no row: {handles:?}"
        );
    }
    assert!(
        field
            .runs
            .iter()
            .any(|run| leaves_a_node_out_in_sight(run, cut.now())),
        "no run in sight has a node its mode leaves out: {handles:?}"
    );
}
