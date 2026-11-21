//! A run's screen as its target's cut makes it: a run at work seen as its last event lands,
//! in a field of older runs, one that needs the person and one stalled; and the findings
//! cut where every finding is posted and none answered.

use std::time::Duration;

use surco::words::Standing;
use yunta_api::{FieldReach, FollowNews, Followed, FollowedField, FollowedRun};
use yunta_core::events::{FindingSeverity, NodeState};
use yunta_replay::frame::NodeStanding;
use yunta_testkit_ui::cut::Cut;

use crate::{design, following, next, read_whole};

/// The field of `cut`'s repository, as the following's first piece of news hands it.
pub(crate) async fn field_here(cut: &Cut) -> FollowedField {
    let followed = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    let mut following = following(cut, followed).await;
    match next(&mut following, "the field read whole").await {
        FollowNews::Field(field) => *field,
        other => panic!("the first piece of news is the field read whole: {other:?}"),
    }
}

/// How long the design's run has gone when `plan` makes its third call, as the design's
/// frame of that moment says it: `for 7m34s`.
const AT_WORK_FOR: Duration = Duration::from_secs(7 * 60 + 34);

#[tokio::test]
async fn a_run_at_work_is_seen_as_its_last_event_lands() {
    let cut = Cut::of_target("work").await;
    let (run, _) = read_whole(&cut, &design()).await;
    let frame = run.frame(cut.now());
    assert_eq!(frame.elapsed, Some(AT_WORK_FOR));
    assert_eq!(frame.since_last_event, Some(Duration::ZERO));
}

#[tokio::test]
async fn a_runs_screen_stands_in_a_field_of_a_run_that_needs_the_person_and_one_stalled_both_older()
{
    let cut = Cut::of_target("work").await;
    let field = field_here(&cut).await;
    let born = |run: &FollowedRun| {
        run.state
            .clock()
            .first_at()
            .expect("a run of the field holds its first event")
    };
    let in_view = field
        .runs
        .iter()
        .find(|run| run.run == design())
        .expect("the run in view stands in its field");
    let beside: Vec<&FollowedRun> = field
        .runs
        .iter()
        .filter(|run| run.run != design())
        .collect();
    let standings: Vec<Standing> = beside
        .iter()
        .map(|run| run.frame(cut.now()).standing)
        .collect();
    assert_eq!(
        standings.len(),
        2,
        "the field beside the run: {standings:?}"
    );
    for standing in [Standing::NeedsYou, Standing::Stalled] {
        assert_eq!(
            standings.iter().filter(|each| **each == standing).count(),
            1,
            "the field beside the run: {standings:?}"
        );
    }
    for run in beside {
        assert!(
            born(run) < born(in_view),
            "{} is born at {}, after the run in view",
            run.run,
            born(run)
        );
    }
}

/// The nodes the design's run reviews its work with, at once.
const REVIEWERS: [&str; 3] = ["review@reviewer", "review@reviewer-alt", "conform"];

#[tokio::test]
async fn the_findings_cut_holds_the_four_findings_one_blocking_none_answered_its_reviewers_at_work()
{
    let cut = Cut::of_target("findings").await;
    let (run, _) = read_whole(&cut, &design()).await;
    let findings = run.state.findings.effective();
    assert_eq!(findings.len(), 4, "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|posted| posted.finding.severity == FindingSeverity::Blocking),
        "no finding blocks: {findings:?}"
    );
    for posted in &findings {
        let answers = run
            .state
            .findings
            .answers(posted.node.as_ref(), &posted.finding.id);
        assert!(answers.is_empty(), "`{}` is answered", posted.finding.id);
    }
    let frame = run.frame(cut.now());
    for reviewer in REVIEWERS {
        let node = frame
            .nodes
            .iter()
            .find(|node| node.id.as_str() == reviewer)
            .expect("a reviewer of the run");
        assert!(
            matches!(node.state, NodeStanding::Reached(NodeState::Running { .. })),
            "`{reviewer}`: {:?}",
            node.state
        );
    }
}
