//! The gate's pull request on GitHub: what its description tells a reviewer,
//! what a later lap redraws, and how a long plan fits in the limit GitHub
//! keeps on a description.

use super::*;
use std::path::PathBuf;
use yunta_core::events::{ArtifactId, Shown};
use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
use yunta_core::{ArtifactKind, ContentHash, TasksFile};

/// GitHub's limit on a pull request's description: 65,536 code points as far as is
/// known, since GitHub documents no count. The body is held to this many UTF-8 bytes,
/// which are never fewer than the code points or UTF-16 units any count would take, so
/// it is within the limit however GitHub counts.
const DESCRIPTION_LIMIT: usize = 65_536;

/// The length of the plan the long-plan test shows: past the limit, so the
/// description cannot hold all of it.
const LONG_PLAN: usize = 70_000;

/// A plan shown beside a gate's question, with `summary` as its summary and
/// `description` as its description.
fn shown_plan(summary: &str, description: String) -> ShownDocument {
    ShownDocument {
        shown: Shown {
            producer: None,
            artifact: ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks,
            },
            content_hash: ContentHash::sha256(description.as_bytes()),
        },
        path: PathBuf::from("artifacts/plan/tasks.yaml"),
        content: ShownContent::Tasks(Box::new(PlanReview {
            plan: TasksFile {
                summary: Some(summary.to_string()),
                description: Some(description),
                ..TasksFile::of(Vec::new())
            },
            departed: Vec::new(),
            spec: None,
            spec_node: None,
            asked: None,
            held: Vec::new(),
            tasks: Vec::new(),
            handed_over: None,
            answered: Vec::new(),
            shapes_read: None,
        })),
    }
}

/// A gate's body says how a review answers it and names the machine that holds
/// the run for the commands. It names neither `yunta cancel`, which stops
/// nothing on a parked run, nor the terminal's `resolve-gate`.
#[tokio::test]
async fn a_gate_s_body_answers_on_github_and_names_the_machine_that_holds_the_run() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);

    forge
        .publish(&request("yunta/run-1/gate", "run-1"))
        .await
        .unwrap();

    let body = stub.with(|s| s.prs[0].body.clone());
    for said in [
        "| **close** | close this pull request | nothing runs; this decision stays open |",
        "`yunta close run-1`",
    ] {
        assert!(body.contains(said), "{said:?} is not in:\n{body}");
    }
    for unsaid in ["yunta cancel", "resolve-gate"] {
        assert!(!body.contains(unsaid), "{unsaid:?} is in:\n{body}");
    }
}

/// A lap that asks what the pull request already asks reuses that pull request
/// and redraws nothing, so it sends no PATCH.
#[tokio::test]
async fn an_unchanged_lap_sends_no_patch() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);
    let lap = request("yunta/run-1/gate", "run-1");

    let first = forge.publish(&lap).await.unwrap();
    let again = forge.publish(&lap).await.unwrap();

    assert_eq!(
        again.number,
        first.number,
        "the second lap reuses the pull request: {:?}",
        stub.requests()
    );
    assert_eq!(
        stub.with(|s| s.prs.len()),
        1,
        "the second lap opens no pull request: {:?}",
        stub.requests()
    );
    assert!(
        !stub.requests().iter().any(|r| r.starts_with("PATCH ")),
        "{:?}",
        stub.requests()
    );
}

/// A lap whose question changed redraws the pull request it reuses with exactly
/// one PATCH, and the pull request's title then shows the new question.
#[tokio::test]
async fn a_lap_with_a_changed_question_patches_its_pull_request_once() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);
    let mut lap = request("yunta/run-1/gate", "run-1");
    forge.publish(&lap).await.unwrap();

    lap.decision.question =
        Claim::Written("Is the spec ready to build on, after the correction?".to_string());
    let again = forge.publish(&lap).await.unwrap();

    assert_eq!(again.number, 1);
    let patches = stub
        .requests()
        .iter()
        .filter(|r| r.starts_with("PATCH /pulls/1"))
        .count();
    assert_eq!(patches, 1, "{:?}", stub.requests());
    let title = stub.with(|s| s.prs[0].title.clone());
    assert_eq!(
        title,
        "Is the spec ready to build on, after the correction?"
    );
}

/// A lap that changes only what its pull request's body says, here the assignee
/// the question is addressed to, redraws the pull request it reuses with exactly
/// one PATCH, and the body then names the new assignee.
#[tokio::test]
async fn a_lap_that_changes_only_its_body_patches_its_pull_request_once() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);
    let mut lap = request("yunta/run-1/gate", "run-1");
    forge.publish(&lap).await.unwrap();

    lap.decision.assignee = "maria".to_string();
    let again = forge.publish(&lap).await.unwrap();

    assert_eq!(again.number, 1);
    let patches = stub
        .requests()
        .iter()
        .filter(|r| r.starts_with("PATCH /pulls/1"))
        .count();
    assert_eq!(patches, 1, "{:?}", stub.requests());
    let body = stub.with(|s| s.prs[0].body.clone());
    assert!(
        body.contains("Gate `approve-spec` waits on this pull request, for maria."),
        "{body}"
    );
}

/// A forge that returns a pull request's description with CRLF line endings holds
/// the text the lap drew, so a lap that draws the same text sends no PATCH.
#[tokio::test]
async fn a_forge_that_returns_crlf_line_endings_is_not_patched_for_the_same_text() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);
    let lap = request("yunta/run-1/gate", "run-1");
    forge.publish(&lap).await.unwrap();
    stub.with(|s| s.prs[0].body = s.prs[0].body.replace('\n', "\r\n"));

    let again = forge.publish(&lap).await.unwrap();

    assert_eq!(again.number, 1);
    assert!(
        !stub.requests().iter().any(|r| r.starts_with("PATCH ")),
        "{:?}",
        stub.requests()
    );
}

/// A plan longer than GitHub's limit is cut down to fit it: the description
/// leaves part of the plan out, and still ends on the run's marker.
#[tokio::test]
async fn a_long_plan_s_body_fits_github_s_limit() {
    let stub = Stub::default();
    let forge = forge_at(serve(stub.clone()).await);
    let description = "word ".repeat(LONG_PLAN / "word ".len());
    assert_eq!(description.len(), LONG_PLAN);
    let mut lap = request("yunta/run-1/gate", "run-1");
    lap.shown = vec![shown_plan("Make it", description)];

    forge.publish(&lap).await.unwrap();

    let body = stub.with(|s| s.prs[0].body.clone());
    assert!(
        body.len() <= DESCRIPTION_LIMIT,
        "the body is {} bytes, past the limit of {DESCRIPTION_LIMIT}",
        body.len()
    );
    assert!(
        body.ends_with("<!-- yunta run_id: run-1 -->"),
        "the body does not end on the run's marker"
    );
}
