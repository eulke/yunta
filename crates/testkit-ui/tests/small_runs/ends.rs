//! The small runs at either end of a run's life.

use yunta_core::events::{EventPayload, Failure, NodeEvent};
use yunta_replay::frame::RunPhase;
use yunta_testkit_ui::small_runs::ends;

use crate::{built_twice, kinds, stand};

#[tokio::test]
async fn a_run_created_has_started_no_node() {
    let run = built_twice(ends::created).await;
    assert_eq!(
        kinds(&run.events(run.run())),
        vec![("run_created".to_string(), None)]
    );
    assert!(matches!(stand(&run).await.frame.phase, RunPhase::Created));
}

#[tokio::test]
async fn a_run_that_finished_with_a_finding_that_blocks_reports_it() {
    let run = built_twice(ends::reported).await;
    let stand = stand(&run).await;
    assert!(
        matches!(stand.frame.phase, RunPhase::Finished),
        "{:?}",
        stand.frame.phase
    );
    let workspace = run.workspace();
    let project = workspace.project().expect("the small run's repository");
    let receipt = project
        .run(run.run())
        .await
        .expect("the small run, opened")
        .receipt()
        .expect("its receipt");
    assert_eq!(receipt.blocking_findings, 1);
}

#[tokio::test]
async fn a_run_whose_failure_ends_it_closes_failed_with_the_failures_evidence() {
    let run = built_twice(ends::failed).await;
    let stand = stand(&run).await;
    assert!(
        matches!(
            &stand.frame.phase,
            RunPhase::Failed {
                failure: Some(Failure::Exited { .. })
            }
        ),
        "{:?}",
        stand.frame.phase
    );
    let events = run.events(run.run());
    let failed = events
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(failed))) => Some(format!("{failed:?}")),
            _ => None,
        })
        .expect("the command's failure is on the log");
    assert!(failed.contains("fixed.txt is missing"), "{failed}");
    assert_eq!(
        kinds(&events).last().map(|(kind, _)| kind.as_str()),
        Some("run_finished")
    );
}

#[tokio::test]
async fn a_run_promoted_closes_for_its_successor() {
    let run = built_twice(ends::promoted).await;
    let stand = stand(&run).await;
    assert!(
        matches!(&stand.frame.phase, RunPhase::Promoted { to: Some(to) } if to.as_str() == "beta"),
        "{:?}",
        stand.frame.phase
    );
    let runs = run.runs();
    assert_eq!(runs.len(), 2, "the run and its successor: {runs:?}");
    let born = run.events(&runs[1]);
    assert!(
        format!("{:?}", born.first().map(|event| &event.body)).contains(run.run().as_str()),
        "the successor is born promoted from the run"
    );
}

#[tokio::test]
async fn a_log_that_stops_making_sense_reads_broken() {
    let run = built_twice(ends::broken).await;
    let stand = stand(&run).await;
    assert!(
        matches!(stand.frame.phase, RunPhase::Broken { .. }),
        "{:?}",
        stand.frame.phase
    );
}
