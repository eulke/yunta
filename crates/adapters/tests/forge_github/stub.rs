//! The stub GitHub's REST API answers from: the pull requests, reviews,
//! refs and contents a test sets up, and the handlers that answer each call
//! the forge makes to them.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use yunta_adapters::GitHubForge;
use yunta_core::events::{Claim, GateFolder};
use yunta_core::port::{GateDecision, PublishRequest};
use yunta_core::{GitHubRepo, NodeId, RunId, Secret};
use yunta_testkit_core::repo::blob_id;

#[derive(Clone)]
pub(super) struct StubPr {
    pub(super) number: u64,
    pub(super) branch: String,
    pub(super) state: &'static str,
    pub(super) merged: bool,
    pub(super) merged_by: Option<&'static str>,
    pub(super) body: String,
    pub(super) head_sha: String,
    pub(super) merge_commit_sha: Option<String>,
    pub(super) title: String,
}

#[derive(Default)]
pub(super) struct StubState {
    /// What `GET /repos/{owner}/{repo}` says the token may do there.
    pub(super) can_push: bool,
    pub(super) prs: Vec<StubPr>,
    pub(super) reviews: HashMap<u64, Vec<Value>>,
    pub(super) comments: HashMap<u64, Vec<Value>>,
    /// Every request the stub answered: `METHOD path?query`.
    pub(super) requests: Vec<String>,
    /// Answer every call with GitHub's rate-limit refusal.
    pub(super) rate_limited: bool,
    /// Answer every call with this refusal.
    pub(super) refusal: Option<(u16, &'static str)>,
    /// What the branch holds, by path, as the Contents API serves it.
    pub(super) files: HashMap<String, Vec<u8>>,
    /// Answer a read of a file the way the API answers one too large to
    /// come inline: its blob id, and no bytes.
    pub(super) over_inline_limit: bool,
}

#[derive(Clone, Default)]
pub(super) struct Stub(Arc<Mutex<StubState>>);

impl Stub {
    pub(super) fn with<T>(&self, f: impl FnOnce(&mut StubState) -> T) -> T {
        f(&mut self.0.lock().unwrap())
    }

    pub(super) fn requests(&self) -> Vec<String> {
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
        "title": pr.title,
        "body": pr.body,
        "head": { "sha": pr.head_sha, "ref": pr.branch },
    })
}

async fn list_pulls(
    State(stub): State<Stub>,
    Path((_, _)): Path<(String, String)>,
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
    Path((_, _)): Path<(String, String)>,
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
            title: body["title"].as_str().unwrap_or_default().to_string(),
        };
        s.prs.push(pr.clone());
        pr
    });
    Ok(Json(pr_json(&pr)))
}

async fn get_pull(
    State(stub): State<Stub>,
    Path((_, _, number)): Path<(String, String, u64)>,
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

/// `PATCH /pulls/{number}`: the title and body the request carries replace the
/// pull request's own, and the answer is the pull request as it now reads.
async fn patch_pull(
    State(stub): State<Stub>,
    Path((_, _, number)): Path<(String, String, u64)>,
    Json(body): Json<Value>,
) -> Answer {
    record(&stub, "PATCH", &format!("/pulls/{number}"), &HashMap::new());
    if let Some(refusal) = refused(&stub) {
        return Err(refusal);
    }
    let pr = stub.with(|s| {
        s.prs.iter_mut().find(|pr| pr.number == number).map(|pr| {
            if let Some(title) = body["title"].as_str() {
                pr.title = title.to_string();
            }
            if let Some(text) = body["body"].as_str() {
                pr.body = text.to_string();
            }
            pr.clone()
        })
    });
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
    Path((_, _, number)): Path<(String, String, u64)>,
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
    Path((_, _, number)): Path<(String, String, u64)>,
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
    Path((_, _, branch)): Path<(String, String, String)>,
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

async fn create_ref(State(stub): State<Stub>, Path((_, _)): Path<(String, String)>) -> Answer {
    record(&stub, "POST", "/git/refs", &HashMap::new());
    Ok(Json(json!({ "ref": "refs/heads/x" })))
}

async fn get_contents(
    State(stub): State<Stub>,
    Path((_, _, path)): Path<(String, String, String)>,
) -> Answer {
    record(&stub, "GET", &format!("/contents/{path}"), &HashMap::new());
    let (stored, over_inline_limit) =
        stub.with(|s| (s.files.get(&path).cloned(), s.over_inline_limit));
    match stored {
        // Over the inline limit the file is named by its blob id, and no
        // bytes come with it.
        Some(content) if over_inline_limit => Ok(Json(json!({
            "sha": blob_id(&content),
            "encoding": "none",
            "content": "",
        }))),
        // Inline, base64 in lines, the way the Contents API sends it.
        Some(content) => {
            let encoded = STANDARD.encode(&content);
            let lines: Vec<&str> = encoded
                .as_bytes()
                .chunks(60)
                .map(|chunk| std::str::from_utf8(chunk).unwrap())
                .collect();
            Ok(Json(json!({
                "sha": blob_id(&content),
                "encoding": "base64",
                "content": lines.join("\n"),
            })))
        }
        None => Err(refuse(StatusCode::NOT_FOUND, HeaderMap::new(), "Not Found")),
    }
}

async fn put_contents(
    State(stub): State<Stub>,
    Path((_, _, path)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Answer {
    record(&stub, "PUT", &format!("/contents/{path}"), &HashMap::new());
    let content = STANDARD.decode(body["content"].as_str().unwrap()).unwrap();
    stub.with(|s| s.files.insert(path.clone(), content));
    Ok(Json(json!({ "content": { "path": path } })))
}

async fn get_repository(State(stub): State<Stub>, Path((_, _)): Path<(String, String)>) -> Answer {
    record(&stub, "GET", "/repository", &HashMap::new());
    if let Some(refusal) = refused(&stub) {
        return Err(refusal);
    }
    let can_push = stub.with(|s| s.can_push);
    Ok(Json(
        json!({ "permissions": { "push": can_push, "pull": true } }),
    ))
}

/// Serves the stub on a loopback port and returns its address.
pub(super) async fn serve(stub: Stub) -> SocketAddr {
    let app = Router::new()
        .route("/repos/{owner}/{repo}", get(get_repository))
        .route(
            "/repos/{owner}/{repo}/pulls",
            get(list_pulls).post(create_pull),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}",
            get(get_pull).patch(patch_pull),
        )
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
            "/repos/{owner}/{repo}/contents/{*path}",
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

pub(super) fn repo() -> GitHubRepo {
    "octo/widgets".parse().unwrap()
}

pub(super) fn forge_at(addr: SocketAddr) -> GitHubForge {
    GitHubForge::configure(
        repo(),
        Secret::new("token".to_string()),
        yunta_testkit_core::spell(),
    )
    .base_url(format!("http://{addr}"))
    .build()
    .unwrap()
}

/// The marker a pull request opened before the comment form carries,
/// which a run still finds it by.
fn marker(run_id: &str) -> String {
    format!("summary\n\n---\nrun_id: `{run_id}`\n")
}

pub(super) fn stub_pr(number: u64, branch: &str, run_id: &str, state: &'static str) -> StubPr {
    StubPr {
        number,
        branch: branch.to_string(),
        state,
        merged: false,
        merged_by: None,
        body: marker(run_id),
        head_sha: format!("{number:040x}"),
        merge_commit_sha: None,
        title: String::new(),
    }
}

pub(super) fn request(branch: &str, run_id: &str) -> PublishRequest {
    let run_id: RunId = run_id.parse().unwrap();
    let folder = GateFolder::of(&run_id, &NodeId::from("approve-spec"));
    PublishRequest {
        branch: branch.to_string(),
        base_branch: "main".to_string(),
        run_id,
        folder,
        decision: GateDecision {
            node: "approve-spec".into(),
            question: Claim::Written("Is the spec ready to build on?".to_string()),
            assignee: "lead".to_string(),
            then: vec!["build".into(), "docs".into()],
            corrected_by: Some("spec".into()),
        },
        artifacts: vec![("spec.md".to_string(), b"# spec".to_vec())],
        shown: Vec::new(),
    }
}
