//! The mock forge's pull requests: what it opens, reuses, reopens and merges,
//! and what each gate publication leaves on the branch.

use super::*;

#[tokio::test]
async fn polling_a_gate_the_forge_never_published_is_a_typed_error() {
    let forge = MockForge::new(MockForgeState::new(), yunta_testkit_core::spell());
    let gate = PullRequestRef {
        url: "https://forge.example/pr/99".to_string(),
        number: 99,
    };
    let error = forge.poll(&gate).await.unwrap_err();
    assert!(
        matches!(error, ForgeError::UnknownGate { number: 99 }),
        "{error:?}"
    );
}

fn gate_request(run_id: &str) -> PublishRequest {
    let run_id: RunId = run_id.parse().unwrap();
    let node: NodeId = "review".into();
    PublishRequest {
        branch: format!("yunta/{run_id}/gate"),
        base_branch: "main".to_string(),
        folder: GateFolder::of(&run_id, &node),
        run_id,
        decision: GateDecision {
            node,
            question: Claim::Written("Is the spec ready?".to_string()),
            assignee: "lead".to_string(),
            then: Vec::new(),
            corrected_by: None,
        },
        artifacts: Vec::new(),
        shown: Vec::new(),
        changed: None,
    }
}

/// A pull request a workflow node opens on a branch of its own, titled and
/// written as given.
fn node_request(title: &str, body: &str) -> PullRequestRequest {
    PullRequestRequest {
        head: "yunta/run-1/ship".to_string(),
        base: "main".to_string(),
        title: title.to_string(),
        body: body.to_string(),
        run_id: "run-1".to_string(),
        receipt: None,
    }
}

#[tokio::test]
async fn the_mock_forge_moves_the_head_only_when_what_it_publishes_changes() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let mut request = gate_request("run-1");
    request.artifacts = vec![("spec.md".to_string(), b"# spec".to_vec())];

    let published = forge.publish(&request).await.unwrap();
    let first = forge.poll(&published).await.unwrap().head_sha;
    forge.publish(&request).await.unwrap();
    assert_eq!(
        forge.poll(&published).await.unwrap().head_sha,
        first,
        "the same files commit nothing"
    );

    request.artifacts = vec![("spec.md".to_string(), b"# spec, corrected".to_vec())];
    forge.publish(&request).await.unwrap();
    assert_ne!(forge.poll(&published).await.unwrap().head_sha, first);
    let pr = state.pull_requests().remove(0);
    // The review gate's folder, written out: a wrong folder rule fails here.
    assert_eq!(
        pr.files[".yunta/gates/run-1/review/spec.md"],
        b"# spec, corrected"
    );
}

#[tokio::test]
async fn the_mock_forge_reuses_only_open_prs() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());

    let first = forge.publish(&gate_request("run-1")).await.unwrap();
    let again = forge.publish(&gate_request("run-1")).await.unwrap();
    assert_eq!(
        again.number, first.number,
        "an open PR for the run is reused"
    );

    // A person closed it: the gate publishes a new one, never the closed
    // one — the same rule the GitHub forge follows.
    state.close("run-1");
    let reopened = forge.publish(&gate_request("run-1")).await.unwrap();
    assert_ne!(reopened.number, first.number);
    assert_eq!(state.pr_number("run-1"), Some(reopened.number));
}

#[tokio::test]
async fn the_mock_forge_reports_a_merged_pr() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let published = forge.publish(&gate_request("run-1")).await.unwrap();

    let merge_sha = state.merge("run-1", &"octocat".into());

    let polled = forge.poll(&published).await.unwrap();
    assert_eq!(
        polled.review,
        ReviewOutcome::Merged {
            by: "octocat".into(),
            merge_sha,
        }
    );
}

/// A gate's pull request that a later lap reuses takes the title and body that lap draws.
#[tokio::test]
async fn a_reused_gate_pull_request_is_drawn_again() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let mut request = gate_request("run-1");
    let first = forge.publish(&request).await.unwrap();
    let before = state.pull_requests().remove(0);
    assert_eq!(before.title, "Is the spec ready?");

    request.decision.question = Claim::Written("Is the spec ready to build on?".to_string());
    let again = forge.publish(&request).await.unwrap();

    let pulls = state.pull_requests();
    assert_eq!(
        again.number, first.number,
        "the open pull request is reused"
    );
    assert_eq!(pulls.len(), 1, "no second pull request is opened");
    assert_eq!(pulls[0].title, "Is the spec ready to build on?");
    assert_ne!(pulls[0].body, before.body, "the body is drawn again");
    assert!(
        pulls[0].body.contains("Is the spec ready to build on?"),
        "{}",
        pulls[0].body
    );
    assert!(
        !pulls[0].body.contains("Is the spec ready?"),
        "{}",
        pulls[0].body
    );
}

/// A pull request a node opens again on its branch keeps the title and body it opened with.
#[tokio::test]
async fn a_reused_pull_request_of_a_node_keeps_what_it_opened_with() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let first = forge
        .open_pull_request(&node_request("Ship it", "the first body"))
        .await
        .unwrap();
    let again = forge
        .open_pull_request(&node_request("Ship it, reworded", "the second body"))
        .await
        .unwrap();

    let pulls = state.pull_requests();
    assert_eq!(
        again.number, first.number,
        "the open pull request is reused"
    );
    assert_eq!(pulls[0].title, "Ship it");
    assert!(
        pulls[0].body.contains("the first body"),
        "{}",
        pulls[0].body
    );
    assert!(
        !pulls[0].body.contains("the second body"),
        "{}",
        pulls[0].body
    );
}

/// A reopened pull request is the run's newest, open again and waiting for a review.
#[tokio::test]
async fn a_reopened_pull_request_waits_for_a_review() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let first = forge.publish(&gate_request("run-1")).await.unwrap();
    state.close("run-1");
    let second = forge.publish(&gate_request("run-1")).await.unwrap();
    state.close("run-1");

    state.reopen("run-1");

    let pulls = state.pull_requests();
    assert_eq!(pulls.len(), 2, "reopening opens no pull request");
    assert!(!pulls[0].open, "an older pull request stays closed");
    assert!(pulls[1].open, "the newest pull request is open again");
    assert_eq!(
        forge.poll(&first).await.unwrap().review,
        ReviewOutcome::Closed
    );
    assert_eq!(
        forge.poll(&second).await.unwrap().review,
        ReviewOutcome::Pending
    );
}

/// Only a closed pull request can be reopened: an open one keeps the review it holds.
#[tokio::test]
async fn reopening_an_open_pull_request_keeps_its_review() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let published = forge.publish(&gate_request("run-1")).await.unwrap();
    state.approve("run-1", &"octocat".into());
    let first = forge.poll(&published).await.unwrap();

    state.reopen("run-1");

    let second = forge.poll(&published).await.unwrap();
    assert_eq!(second.review, first.review);
    assert_eq!(
        second.review,
        ReviewOutcome::Approved {
            by: "octocat".into(),
            reviewed_sha: first.head_sha,
        }
    );
}

/// A merged pull request is final: it keeps its merge, and the next publish opens a new one.
#[tokio::test]
async fn a_merged_pull_request_is_final() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let merged = forge.publish(&gate_request("run-1")).await.unwrap();
    let merge_sha = state.merge("run-1", &"octocat".into());

    let next = forge.publish(&gate_request("run-1")).await.unwrap();

    assert_eq!(
        forge.poll(&merged).await.unwrap().review,
        ReviewOutcome::Merged {
            by: "octocat".into(),
            merge_sha,
        }
    );
    assert_ne!(
        next.number, merged.number,
        "a merged pull request is never reused"
    );
}

/// A reopen never moves a merged pull request: it stays merged and closed.
#[tokio::test]
async fn a_merged_pull_request_is_never_reopened() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let merged = forge.publish(&gate_request("run-1")).await.unwrap();
    let merge_sha = state.merge("run-1", &"octocat".into());

    state.reopen("run-1");

    assert!(
        !state.pull_requests()[0].open,
        "a merged pull request stays closed"
    );
    assert_eq!(
        forge.poll(&merged).await.unwrap().review,
        ReviewOutcome::Merged {
            by: "octocat".into(),
            merge_sha,
        }
    );
}

/// A merge is final: a merged pull request stays merged whatever a person does
/// to it after.
#[tokio::test]
async fn a_merged_pull_request_stays_merged_whatever_is_done_to_it() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let merged = forge.publish(&gate_request("run-1")).await.unwrap();
    let merge_sha = state.merge("run-1", &"octocat".into());

    state.close("run-1");
    state.approve("run-1", &"hubot".into());
    state.request_changes("run-1", &"hubot".into(), Vec::new());
    state.reopen("run-1");

    assert!(
        !state.pull_requests()[0].open,
        "a merged pull request stays closed"
    );
    assert_eq!(
        forge.poll(&merged).await.unwrap().review,
        ReviewOutcome::Merged {
            by: "octocat".into(),
            merge_sha,
        }
    );
}

/// A description longer than the forge keeps, even at its last level, is
/// refused before anything is published: the forge holds no pull request.
#[tokio::test]
async fn a_description_past_what_the_forge_keeps_is_refused_before_anything_is_published() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone(), yunta_testkit_core::spell());
    let mut request = gate_request("run-1");
    // The decision lists every file the pull request carries, at every level,
    // so enough of them outgrow the description.
    request.artifacts = (0..2_000)
        .map(|n| (format!("notes-{n:04}-{}.md", "x".repeat(40)), b"x".to_vec()))
        .collect();

    let error = forge.publish(&request).await.unwrap_err();

    assert!(
        matches!(error, ForgeError::BodyTooLong { limit: 65_536, .. }),
        "{error:?}"
    );
    assert!(state.pull_requests().is_empty(), "nothing is published");
}
