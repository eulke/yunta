//! The mock forge opens pull requests the way the GitHub forge does: one
//! open pull request per run and branch, reused until someone closes it.

use yunta_adapters::{MockForge, MockForgeState};
use yunta_core::port::{Forge, PullRequestRequest};

fn opening(head: &str, run_id: &str) -> PullRequestRequest {
    PullRequestRequest {
        head: head.to_string(),
        base: "main".to_string(),
        title: "add dark mode".to_string(),
        body: "What the run changed.".to_string(),
        run_id: run_id.to_string(),
        receipt: None,
    }
}

#[tokio::test]
async fn the_mock_opens_one_pr_per_run_branch() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone());

    let first = forge
        .open_pull_request(&opening("yunta/run/r1", "r1"))
        .await
        .unwrap();
    let again = forge
        .open_pull_request(&opening("yunta/run/r1", "r1"))
        .await
        .unwrap();
    assert_eq!(first, again, "a rerun reuses the open pull request");

    state.close("r1");
    let reopened = forge
        .open_pull_request(&opening("yunta/run/r1", "r1"))
        .await
        .unwrap();
    assert_ne!(reopened.number, first.number, "a closed one is not reused");

    let opened = state.pull_requests();
    assert_eq!(opened.len(), 2);
    assert_eq!(
        (
            opened[1].head.as_str(),
            opened[1].base.as_str(),
            opened[1].title.as_str()
        ),
        ("yunta/run/r1", "main", "add dark mode")
    );
    assert!(forge.probe().await.unwrap().can_push.unwrap_or(false));
}
