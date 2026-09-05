//! `GitHubForge` against a local stub of GitHub's REST API: every call
//! it makes is answered here, so the forge's own reading of the API —
//! which pull request it reuses, what a merged one means, how far it
//! reads a list, what a refusal or a silence becomes — is tested
//! without a token, a repository or the network.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use yunta_adapters::{
    Forge, ForgeError, GitHubForge, PublishRequest, PublishedGate, ReviewOutcome,
};
use yunta_core::{GitHubRepo, Secret};

#[derive(Clone)]
struct StubPr {
    number: u64,
    branch: String,
    state: &'static str,
    merged: bool,
    merged_by: Option<&'static str>,
    body: String,
    head_sha: String,
    merge_commit_sha: Option<String>,
}

#[derive(Default)]
struct StubState {
    prs: Vec<StubPr>,
    reviews: HashMap<u64, Vec<Value>>,
    comments: HashMap<u64, Vec<Value>>,
    /// Every request the stub answered: `METHOD path?query`.
    requests: Vec<String>,
    /// Answer every call with GitHub's rate-limit refusal.
    rate_limited: bool,
    /// Answer every call with this refusal.
    refusal: Option<(u16, &'static str)>,
}

#[derive(Clone, Default)]
struct Stub(Arc<Mutex<StubState>>);

impl Stub {
    fn with<T>(&self, f: impl FnOnce(&mut StubState) -> T) -> T {
        f(&mut self.0.lock().unwrap())
    }

    fn requests(&self) -> Vec<String> {
        self.with(|s| s.requests.clone())
    }
}

fn record(stub: &Stub, method: &str, path: &str, query: &HashMap<String, String>) {
    let mut pairs: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
    pairs.sort();
    stub.with(|s| {
        s.requests
            .push(format!("{method} {path}?{}", pairs.join("&")))
    });
}

/// A refusal the stub answers with: status, headers and body, boxed so
/// the handlers' `Result` stays small.
struct Refusal(Box<(StatusCode, HeaderMap, String)>);

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (*self.0).into_response()
    }
}

fn refuse(status: StatusCode, headers: HeaderMap, body: &str) -> Refusal {
    Refusal(Box::new((status, headers, body.to_string())))
}

type Answer = Result<Json<Value>, Refusal>;

fn refused(stub: &Stub) -> Option<Refusal> {
    stub.with(|s| {
        if s.rate_limited {
            let mut headers = HeaderMap::new();
            headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
            headers.insert("retry-after", "7".parse().unwrap());
            return Some(refuse(
                StatusCode::FORBIDDEN,
                headers,
                "API rate limit exceeded",
            ));
        }
        s.refusal.map(|(status, body)| {
            refuse(
                StatusCode::from_u16(status).unwrap(),
                HeaderMap::new(),
                body,
            )
        })
    })
}

fn pr_json(pr: &StubPr) -> Value {
    json!({
        "number": pr.number,
        "html_url": format!("https://github.example/pr/{}", pr.number),
        "state": pr.state,
        "merged": pr.merged,
        "merged_by": pr.merged_by.map(|login| json!({ "login": login })),
        "merge_commit_sha": pr.merge_commit_sha,
        "body": pr.body,
        "head": { "sha": pr.head_sha, "ref": pr.branch },
    })
}

async fn list_pulls(
    State(stub): State<Stub>,
    Path((_owner, _repo)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
) -> Answer {
    record(&stub, "GET", "/pulls", &query);
    if let Some(refusal) = refused(&stub) {
        return Err(refusal);
    }
    let head = query.get("head").cloned().unwrap_or_default();
    let branch = head
        .split_once(':')
        .map(|(_, b)| b.to_string())
        .unwrap_or(head);
    let state = query
        .get("state")
        .cloned()
        .unwrap_or_else(|| "open".to_string());
    let prs = stub.with(|s| {
        s.prs
            .iter()
            .filter(|pr| pr.branch == branch && (state == "all" || pr.state == state))
            .map(pr_json)
            .collect::<Vec<_>>()
    });
    Ok(Json(Value::Array(prs)))
}

async fn create_pull(
    State(stub): State<Stub>,
    Path((_owner, _repo)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Answer {
    record(&stub, "POST", "/pulls", &HashMap::new());
    if let Some(refusal) = refused(&stub) {
        return Err(refusal);
    }
    let pr = stub.with(|s| {
        let number = s.prs.iter().map(|pr| pr.number).max().unwrap_or(0) + 1;
        let pr = StubPr {
            number,
            branch: body["head"].as_str().unwrap_or_default().to_string(),
            state: "open",
            merged: false,
            merged_by: None,
            body: body["body"].as_str().unwrap_or_default().to_string(),
            head_sha: format!("{number:040x}"),
            merge_commit_sha: None,
        };
        s.prs.push(pr.clone());
        pr
    });
    Ok(Json(pr_json(&pr)))
}

async fn get_pull(
    State(stub): State<Stub>,
    Path((_owner, _repo, number)): Path<(String, String, u64)>,
) -> Answer {
    record(&stub, "GET", &format!("/pulls/{number}"), &HashMap::new());
    if let Some(refusal) = refused(&stub) {
        return Err(refusal);
    }
    let pr = stub.with(|s| s.prs.iter().find(|pr| pr.number == number).cloned());
    match pr {
        Some(pr) => Ok(Json(pr_json(&pr))),
        None => Err(refuse(StatusCode::NOT_FOUND, HeaderMap::new(), "Not Found")),
    }
}

/// One page of `items`, the way GitHub pages: `per_page` and `page`
/// (1-based) from the query.
fn page(items: &[Value], query: &HashMap<String, String>) -> Vec<Value> {
    let per_page: usize = query
        .get("per_page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let page: usize = query.get("page").and_then(|v| v.parse().ok()).unwrap_or(1);
    items
        .iter()
        .skip((page - 1) * per_page)
        .take(per_page)
        .cloned()
        .collect()
}

async fn list_reviews(
    State(stub): State<Stub>,
    Path((_owner, _repo, number)): Path<(String, String, u64)>,
    Query(query): Query<HashMap<String, String>>,
) -> Answer {
    record(&stub, "GET", &format!("/pulls/{number}/reviews"), &query);
    if let Some(refusal) = refused(&stub) {
        return Err(refusal);
    }
    let items = stub.with(|s| s.reviews.get(&number).cloned().unwrap_or_default());
    Ok(Json(Value::Array(page(&items, &query))))
}

async fn list_comments(
    State(stub): State<Stub>,
    Path((_owner, _repo, number)): Path<(String, String, u64)>,
    Query(query): Query<HashMap<String, String>>,
) -> Answer {
    record(&stub, "GET", &format!("/pulls/{number}/comments"), &query);
    if let Some(refusal) = refused(&stub) {
        return Err(refusal);
    }
    let items = stub.with(|s| s.comments.get(&number).cloned().unwrap_or_default());
    Ok(Json(Value::Array(page(&items, &query))))
}

async fn get_ref(
    State(stub): State<Stub>,
    Path((_owner, _repo, branch)): Path<(String, String, String)>,
) -> Answer {
    record(
        &stub,
        "GET",
        &format!("/git/ref/heads/{branch}"),
        &HashMap::new(),
    );
    if let Some(refusal) = refused(&stub) {
        return Err(refusal);
    }
    Ok(Json(json!({ "object": { "sha": "base-sha" } })))
}

async fn create_ref(
    State(stub): State<Stub>,
    Path((_owner, _repo)): Path<(String, String)>,
) -> Answer {
    record(&stub, "POST", "/git/refs", &HashMap::new());
    Ok(Json(json!({ "ref": "refs/heads/x" })))
}

async fn get_contents(
    State(stub): State<Stub>,
    Path((_owner, _repo, path)): Path<(String, String, String)>,
) -> Answer {
    record(&stub, "GET", &format!("/contents/{path}"), &HashMap::new());
    Err(refuse(StatusCode::NOT_FOUND, HeaderMap::new(), "Not Found"))
}

async fn put_contents(
    State(stub): State<Stub>,
    Path((_owner, _repo, path)): Path<(String, String, String)>,
) -> Answer {
    record(&stub, "PUT", &format!("/contents/{path}"), &HashMap::new());
    Ok(Json(json!({ "content": { "path": path } })))
}

/// Serves the stub on a loopback port and returns its address.
async fn serve(stub: Stub) -> SocketAddr {
    let app = Router::new()
        .route(
            "/repos/{owner}/{repo}/pulls",
            get(list_pulls).post(create_pull),
        )
        .route("/repos/{owner}/{repo}/pulls/{number}", get(get_pull))
        .route(
            "/repos/{owner}/{repo}/pulls/{number}/reviews",
            get(list_reviews),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}/comments",
            get(list_comments),
        )
        .route("/repos/{owner}/{repo}/git/ref/heads/{branch}", get(get_ref))
        .route("/repos/{owner}/{repo}/git/refs", post(create_ref))
        .route(
            "/repos/{owner}/{repo}/contents/{path}",
            get(get_contents).put(put_contents),
        )
        .with_state(stub);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn repo() -> GitHubRepo {
    "octo/widgets".parse().unwrap()
}

fn forge_at(addr: SocketAddr) -> GitHubForge {
    GitHubForge::configure(repo(), Secret::new("token".to_string()))
        .base_url(format!("http://{addr}"))
        .build()
        .unwrap()
}

fn marker(run_id: &str) -> String {
    format!("summary\n\n---\nrun_id: `{run_id}`\n")
}

fn stub_pr(number: u64, branch: &str, run_id: &str, state: &'static str) -> StubPr {
    StubPr {
        number,
        branch: branch.to_string(),
        state,
        merged: false,
        merged_by: None,
        body: marker(run_id),
        head_sha: format!("{number:040x}"),
        merge_commit_sha: None,
    }
}

fn request(branch: &str, run_id: &str) -> PublishRequest {
    PublishRequest {
        branch: branch.to_string(),
        base_branch: "main".to_string(),
        run_id: run_id.to_string(),
        summary: "spec ready for review".to_string(),
        artifacts: vec![("spec.md".to_string(), b"# spec".to_vec())],
    }
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
        body.contains("run_id: `run-1`"),
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
        .poll(&PublishedGate {
            url: "https://github.example/pr/7".to_string(),
            number: 7,
        })
        .await
        .unwrap();

    assert_eq!(
        polled.review,
        ReviewOutcome::Merged {
            by: "octocat".to_string(),
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
        .poll(&PublishedGate {
            url: "https://github.example/pr/3".to_string(),
            number: 3,
        })
        .await
        .unwrap();

    assert_eq!(
        polled.review,
        ReviewOutcome::Approved {
            by: "late-approver".to_string(),
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
        .poll(&PublishedGate {
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
        .poll(&PublishedGate {
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
