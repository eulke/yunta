//! Where the run puts its pointers: a node at work keeps the node pointer while it works,
//! though a node declared before it starts; and the finding pointer stands on the first
//! finding no node answered, else on the last, as the design's run posts and answers its
//! four.

use yunta_api::{FollowNews, Followed, Stop};
use yunta_core::events::findings::FindingLedger;
use yunta_core::events::{NodeOutcome, NodeState, TokenUsage};
use yunta_core::{FindingId, NodeId, RunId};
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame};
use yunta_replay::RunState;
use yunta_testkit_ui::cut::{Built, Cut, Shown};

use super::{finding_led, node_led};

/// A node of a run's frame, in `state`.
fn node(id: &str, state: NodeState) -> NodeFrame {
    NodeFrame::sample(&NodeId::from(id), NodeStanding::Reached(state))
}

/// A node at work on its first attempt.
fn at_work(id: &str) -> NodeFrame {
    node(id, NodeState::Running { attempt: 1 })
}

/// A node that finished.
fn finished(id: &str) -> NodeFrame {
    let outcome = NodeOutcome::Summary("done".to_string());
    let tokens = TokenUsage::default();
    node(id, NodeState::Finished { outcome, tokens })
}

/// A running run's frame of `nodes`, in declared order.
fn running(nodes: Vec<NodeFrame>) -> RunFrame {
    RunFrame {
        nodes,
        ..RunFrame::sample(&RunId::from("01M7LEAD000000000000000000"))
    }
}

#[test]
fn a_node_at_work_keeps_the_pointer_while_it_works_though_one_declared_before_it_starts() {
    let (early, late) = (NodeId::from("early"), NodeId::from("late"));
    let state = RunState::default();
    let both = running(vec![at_work("early"), at_work("late")]);
    assert_eq!(
        node_led(&both, &state, Some(&late)),
        Some(late.clone()),
        "kept where it stands while that node works"
    );
    assert_eq!(
        node_led(&both, &state, None),
        Some(early.clone()),
        "put first on the first at work in declared order"
    );
    let ended = running(vec![at_work("early"), finished("late")]);
    assert_eq!(
        node_led(&ended, &state, Some(&late)),
        Some(early),
        "once it ends, the first at work"
    );
}

/// The findings the design's run holds at `seq`.
async fn findings_at(seq: u64) -> FindingLedger {
    let cut = Cut::of(&[Shown::at(Built::Designed, seq)]).await;
    let project = cut.workspace().project().expect("the cut's repository");
    let followed = Followed {
        field: None,
        run: Some(cut.runs()[0].clone()),
    };
    let mut following = project.follow(followed).await.expect("the design's run");
    match following.next(&Stop::new()).await {
        Ok(FollowNews::Whole { run, .. }) => run.state().findings.clone(),
        other => panic!("the run read whole comes first: {other:?}"),
    }
}

/// A finding by the node that posted it and its id.
fn posted(node: &'static str, id: &'static str) -> Option<(Option<NodeId>, FindingId)> {
    Some((Some(NodeId::from_static(node)), FindingId::from_static(id)))
}

#[tokio::test]
async fn the_finding_pointer_stands_on_the_first_finding_no_node_answered_else_on_the_last() {
    // Its four findings posted, none answered: the first posted.
    let none_answered = findings_at(200).await;
    assert_eq!(
        finding_led(&none_answered),
        posted("review@reviewer", "global-paths-ignore-yunta-home")
    );
    // `conform`'s and the first reviewer's answered: the first posted of the other two.
    let two_answered = findings_at(225).await;
    assert_eq!(
        finding_led(&two_answered),
        posted("review@reviewer-alt", "global-commands-ignore-yunta-home")
    );
    // Every one answered: the last posted.
    let all_answered = findings_at(229).await;
    assert_eq!(
        finding_led(&all_answered),
        posted("conform", "target-outside-declared-file")
    );
}
