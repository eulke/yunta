//! `GitHubForge` against a local stub of GitHub's REST API: every call
//! it makes is answered here, so the forge's own reading of the API —
//! which pull request it reuses, what a merged one means, how far it
//! reads a list, what a refusal or a silence becomes — is tested
//! without a token, a repository or the network.

use std::time::Duration;

use serde_json::{json, Value};
use yunta_adapters::GitHubForge;
use yunta_core::events::Claim;
use yunta_core::port::{
    Forge, ForgeError, ForgeProbe, PullRequestRef, PullRequestRequest, ReviewOutcome,
};
use yunta_core::Secret;

#[path = "forge_github/gate_page.rs"]
mod gate_page;
#[path = "forge_github/stub.rs"]
mod stub;

use stub::*;

/// Every file the stub was asked to commit, in order.
fn commits(stub: &Stub) -> Vec<String> {
    stub.requests()
        .into_iter()
        .filter(|r| r.starts_with("PUT /contents/"))
        .collect()
}

#[tokio::test]
async fn a_gate_pull_request_asks_its_question_and_says_what_each_review_does() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);

    forge
        .publish(&request("yunta/run-1/gate", "run-1"))
        .await
        .unwrap();

    let pr = stub.with(|s| s.prs[0].clone());
    assert_eq!(pr.title, "Is the spec ready to build on?");
    for said in [
        "Gate `approve-spec` waits on this pull request, for lead.",
        "| **approve** | approve this pull request, or merge it | resolves this gate; `build`, `docs` run next |",
        "each comment reaches `spec` as a finding",
        "| **close** | close this pull request | nothing runs; this decision stays open |",
        "`.yunta/gates/run-1/approve-spec/spec.md`: as the run holds it",
        "`yunta resume run-1`",
    ] {
        assert!(pr.body.contains(said), "{said:?} is not in:\n{}", pr.body);
    }
    for unsaid in ["yunta cancel", "resolve-gate"] {
        assert!(!pr.body.contains(unsaid), "{unsaid:?} is in:\n{}", pr.body);
    }
    assert!(
        pr.body.ends_with("<!-- yunta run_id: run-1 -->"),
        "{}",
        pr.body
    );
}

#[tokio::test]
async fn a_later_lap_commits_only_what_changed_to_the_pull_request_it_reuses() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);
    let mut lap = request("yunta/run-1/gate", "run-1");

    let first = forge.publish(&lap).await.unwrap();
    assert_eq!(
        commits(&stub),
        ["PUT /contents/.yunta/gates/run-1/approve-spec/spec.md?"]
    );

    let again = forge.publish(&lap).await.unwrap();
    assert_eq!(again.number, first.number);
    assert_eq!(
        commits(&stub).len(),
        1,
        "a file the branch already holds is not committed again"
    );

    lap.artifacts = vec![("spec.md".to_string(), b"# spec, corrected".to_vec())];
    let corrected = forge.publish(&lap).await.unwrap();
    assert_eq!(corrected.number, first.number);
    assert_eq!(commits(&stub).len(), 2);
    assert_eq!(
        stub.with(|s| s.files[".yunta/gates/run-1/approve-spec/spec.md"].clone()),
        b"# spec, corrected"
    );
}

#[tokio::test]
async fn an_unchanged_file_over_the_inline_limit_is_not_committed_again() {
    let stub = Stub::default();
    stub.with(|s| s.over_inline_limit = true);
    let forge = forge_at(serve(stub.clone()).await);
    let lap = request("yunta/run-1/gate", "run-1");
    forge.publish(&lap).await.unwrap();
    forge.publish(&lap).await.unwrap();
    assert_eq!(
        commits(&stub).len(),
        1,
        "a file the branch holds is not committed again"
    );
}

#[tokio::test]
async fn publish_reuses_only_open_prs() {
    let stub = Stub::default();
    // A closed PR on the branch, carrying this run's marker: a person
    // closed it, and it must never keep the gate from publishing again.
    stub.with(|s| {
        s.prs
            .push(stub_pr(1, "yunta/run-1/gate", "run-1", "closed"))
    });
    let forge = forge_at(serve(stub.clone()).await);

    let published = forge
        .publish(&request("yunta/run-1/gate", "run-1"))
        .await
        .unwrap();

    assert_eq!(
        published.number, 2,
        "a closed PR is not reused: {published:?}"
    );
    assert!(
        stub.requests()
            .iter()
            .any(|r| r.starts_with("GET /pulls?") && r.contains("state=open")),
        "the search asks for open PRs only: {:?}",
        stub.requests()
    );
    let body = stub.with(|s| s.prs[1].body.clone());
    assert!(
        body.contains("<!-- yunta run_id: run-1 -->"),
        "the new PR carries the run marker: {body}"
    );

    // The open PR with the marker is the one to reuse.
    let published_again = forge
        .publish(&request("yunta/run-1/gate", "run-1"))
        .await
        .unwrap();
    assert_eq!(published_again.number, 2);
}

#[tokio::test]
async fn an_open_pr_without_the_run_marker_is_not_reused() {
    let stub = Stub::default();
    stub.with(|s| {
        let mut pr = stub_pr(1, "yunta/run-1/gate", "someone-else", "open");
        pr.body = "a person's own pull request".to_string();
        s.prs.push(pr);
    });
    let forge = forge_at(serve(stub.clone()).await);

    let published = forge
        .publish(&request("yunta/run-1/gate", "run-1"))
        .await
        .unwrap();

    assert_eq!(published.number, 2);
}

/// The merge commit the stub reports for a merged pull request.
const MERGE_SHA: &str = "0000000000000000000000000000000000feed00";

#[tokio::test]
async fn merged_pr_is_not_closed() {
    let stub = Stub::default();
    stub.with(|s| {
        let mut pr = stub_pr(7, "yunta/run-1/gate", "run-1", "closed");
        pr.merged = true;
        pr.merged_by = Some("octocat");
        pr.merge_commit_sha = Some(MERGE_SHA.to_string());
        s.prs.push(pr);
    });
    let forge = forge_at(serve(stub.clone()).await);

    let polled = forge
        .poll(&PullRequestRef {
            url: "https://github.example/pr/7".to_string(),
            number: 7,
        })
        .await
        .unwrap();

    assert_eq!(
        polled.review,
        ReviewOutcome::Merged {
            by: "octocat".into(),
            merge_sha: MERGE_SHA.parse().unwrap(),
        }
    );
}

#[tokio::test]
async fn reviews_are_paginated() {
    let stub = Stub::default();
    stub.with(|s| {
        s.prs.push(stub_pr(3, "yunta/run-1/gate", "run-1", "open"));
        let mut reviews: Vec<Value> = (0..100)
            .map(|i| json!({ "state": "COMMENTED", "user": { "login": format!("c{i}") }, "commit_id": format!("{:040x}", 3) }))
            .collect();
        reviews.push(json!({ "state": "APPROVED", "user": { "login": "late-approver" }, "commit_id": format!("{:040x}", 3) }));
        s.reviews.insert(3, reviews);
    });
    let forge = forge_at(serve(stub.clone()).await);

    let polled = forge
        .poll(&PullRequestRef {
            url: "https://github.example/pr/3".to_string(),
            number: 3,
        })
        .await
        .unwrap();

    assert_eq!(
        polled.review,
        ReviewOutcome::Approved {
            by: "late-approver".into(),
            reviewed_sha: format!("{:040x}", 3).parse().unwrap(),
        },
        "the approval on the second page is found"
    );
    assert!(
        stub.requests()
            .iter()
            .any(|r| r.contains("/reviews?") && r.contains("page=2")),
        "the second page was read: {:?}",
        stub.requests()
    );
}

#[tokio::test]
async fn a_rate_limit_is_its_own_error_with_the_wait() {
    let stub = Stub::default();
    stub.with(|s| s.rate_limited = true);
    let forge = forge_at(serve(stub.clone()).await);

    let error = forge
        .publish(&request("yunta/run-1/gate", "run-1"))
        .await
        .unwrap_err();

    match error {
        ForgeError::RateLimited { retry_after, .. } => {
            assert_eq!(retry_after, Some(Duration::from_secs(7)));
        }
        other => panic!("expected a rate limit, got {other:?}"),
    }
}

#[tokio::test]
async fn a_refusal_keeps_its_status_and_body() {
    let stub = Stub::default();
    stub.with(|s| s.refusal = Some((422, "Validation Failed: base branch does not exist")));
    let forge = forge_at(serve(stub.clone()).await);

    let error = forge
        .publish(&request("yunta/run-1/gate", "run-1"))
        .await
        .unwrap_err();

    match error {
        ForgeError::Http { status, body, .. } => {
            assert_eq!(status, 422);
            assert_eq!(body, "Validation Failed: base branch does not exist");
        }
        other => panic!("expected an HTTP refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unanswered_request_is_a_transport_error() {
    // A port that accepts the connection and never answers.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let forge = GitHubForge::configure(repo(), Secret::new("token".to_string()))
        .base_url(format!("http://{addr}"))
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();

    let error = forge
        .poll(&PullRequestRef {
            url: "https://github.example/pr/1".to_string(),
            number: 1,
        })
        .await
        .unwrap_err();

    assert!(
        matches!(error, ForgeError::Transport { .. }),
        "got: {error:?}"
    );
    drop(listener);
}

#[tokio::test]
async fn a_review_without_a_commit_id_is_not_a_decision() {
    let stub = Stub::default();
    stub.with(|s| {
        s.prs.push(stub_pr(4, "yunta/run-1/gate", "run-1", "open"));
        s.reviews.insert(
            4,
            vec![json!({ "state": "APPROVED", "user": { "login": "octocat" } })],
        );
    });
    let forge = forge_at(serve(stub.clone()).await);

    let polled = forge
        .poll(&PullRequestRef {
            url: "https://github.example/pr/4".to_string(),
            number: 4,
        })
        .await
        .unwrap();

    assert_eq!(
        polled.review,
        ReviewOutcome::Pending,
        "an approval that names no commit covers no code, so the gate keeps waiting"
    );
}

fn pull_request(head: &str, run_id: &str) -> PullRequestRequest {
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
async fn open_pull_request_reuses_the_open_pr_carrying_the_marker() {
    let stub = Stub::default();
    stub.with(|s| s.prs.push(stub_pr(4, "yunta/run/run-1", "run-1", "open")));
    let forge = forge_at(serve(stub.clone()).await);

    let opened = forge
        .open_pull_request(&pull_request("yunta/run/run-1", "run-1"))
        .await
        .unwrap();

    assert_eq!(opened.number, 4);
    assert!(
        !stub.requests().iter().any(|r| r.starts_with("POST /pulls")),
        "{:?}",
        stub.requests()
    );
}

#[tokio::test]
async fn open_pull_request_posts_title_body_and_marker() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);

    let opened = forge
        .open_pull_request(&pull_request("yunta/run/run-1", "run-1"))
        .await
        .unwrap();

    let pr = stub.with(|s| s.prs[0].clone());
    assert_eq!(opened.number, pr.number);
    assert_eq!(pr.title, "add dark mode");
    assert_eq!(pr.branch, "yunta/run/run-1");
    assert_eq!(
        pr.body,
        "What the run changed.\n\n<!-- yunta run_id: run-1 -->"
    );
}

#[tokio::test]
async fn an_open_pull_request_carrying_the_comment_marker_is_reused() {
    let stub = Stub::default();
    stub.with(|s| {
        let mut pr = stub_pr(4, "yunta/run/run-1", "run-1", "open");
        pr.body = "What the run changed.\n\n<!-- yunta run_id: run-1 -->".to_string();
        s.prs.push(pr);
    });
    let forge = forge_at(serve(stub.clone()).await);

    let opened = forge
        .open_pull_request(&pull_request("yunta/run/run-1", "run-1"))
        .await
        .unwrap();

    assert_eq!(opened.number, 4, "{:?}", stub.requests());
}

#[tokio::test]
async fn a_closed_pr_is_not_reused_when_opening_one() {
    let stub = Stub::default();
    stub.with(|s| s.prs.push(stub_pr(1, "yunta/run/run-1", "run-1", "closed")));
    let forge = forge_at(serve(stub.clone()).await);

    let opened = forge
        .open_pull_request(&pull_request("yunta/run/run-1", "run-1"))
        .await
        .unwrap();

    assert_eq!(opened.number, 2);
}

#[tokio::test]
async fn the_probe_says_whether_the_token_can_push() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);
    assert_eq!(
        forge.probe().await.unwrap(),
        ForgeProbe {
            can_push: Some(false)
        }
    );
    stub.with(|s| s.can_push = true);
    assert_eq!(
        forge.probe().await.unwrap(),
        ForgeProbe {
            can_push: Some(true)
        }
    );
}

#[tokio::test]
async fn the_probe_reports_a_refused_token() {
    let stub = Stub::default();
    stub.with(|s| s.refusal = Some((401, "Bad credentials")));
    let forge = forge_at(serve(stub.clone()).await);

    let refused = forge.probe().await.unwrap_err();

    assert!(
        matches!(&refused, ForgeError::Http { status, .. } if *status == 401),
        "{refused:?}"
    );
}
