//! `GitHubForge`: the only real `Forge` in v1. Talks to GitHub's REST
//! API directly (no local git) — the Contents/Refs endpoints can create
//! a branch and commit files over plain HTTP, so publishing a gate
//! needs nothing beyond the same bearer token that reads its PR state
//! back.
//!
//! Every endpoint and field used here is the documented GitHub REST API
//! v3 shape (`git/refs`, `contents`, `pulls`, `pulls/.../reviews`,
//! `pulls/.../comments`), and `crates/adapters/tests/forge_github.rs`
//! drives this client against a local stub that answers in that shape.
//! What no test here can tell is whether the live API still answers
//! that way; a check run by hand against the real forge covers it, and
//! cargo test runs the stubs only.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use yunta_core::commit_message::CommitMessage;
use yunta_core::{CommitSha, GitHubRepo, Responder, Secret};

use yunta_core::port::{
    Forge, ForgeError, ForgeProbe, PolledGate, PublishRequest, PullRequestRef, PullRequestRequest,
    ReviewComment,
};

mod commit;
mod poll;

const API_VERSION: &str = "2022-11-28";

/// How long one request may take, connection included, before it
/// counts as unanswered.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the connection alone may take: a forge that does not
/// accept a connection in this time is down.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// GitHub's largest page. A list is read page after page until a page
/// comes back short.
const PAGE_SIZE: usize = 100;

pub struct GitHubForge {
    client: reqwest::Client,
    repo: GitHubRepo,
    token: Secret<String>,
    base_url: String,
}

/// How a [`GitHubForge`] is built: the public API and the default
/// patience unless told otherwise — a test points it at a local stub
/// and shortens the wait.
pub struct GitHubForgeBuilder {
    repo: GitHubRepo,
    token: Secret<String>,
    base_url: String,
    timeout: Duration,
}

impl GitHubForgeBuilder {
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// The whole request's patience, [`REQUEST_TIMEOUT`] by default.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> Result<GitHubForge, ForgeError> {
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|source| ForgeError::Transport {
                action: "build the HTTP client",
                source: Box::new(source),
            })?;
        Ok(GitHubForge {
            client,
            repo: self.repo,
            token: self.token,
            base_url: self.base_url.trim_end_matches('/').to_string(),
        })
    }
}

/// The open pull request a run's gate already holds: its reference, and the
/// title and description the forge holds for it.
pub(super) struct OpenPr {
    pub(super) reference: PullRequestRef,
    pub(super) title: String,
    pub(super) body: String,
}

impl GitHubForge {
    /// A builder for a forge on `repo`, which `token` reaches.
    pub fn configure(repo: GitHubRepo, token: Secret<String>) -> GitHubForgeBuilder {
        GitHubForgeBuilder {
            repo,
            token,
            base_url: "https://api.github.com".to_string(),
            timeout: REQUEST_TIMEOUT,
        }
    }

    /// Against the public API, with the default patience.
    pub fn new(repo: GitHubRepo, token: Secret<String>) -> Result<Self, ForgeError> {
        Self::configure(repo, token).build()
    }

    pub(super) fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(self.token.expose())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", "yunta")
    }

    /// `/repos/{owner}/{name}{rest}`.
    pub(super) fn repo_path(&self, rest: &str) -> String {
        format!("/repos/{}/{}{rest}", self.repo.owner(), self.repo.name())
    }

    /// Sends the request and reads its answer as a success, a rate
    /// limit or a refusal; no answer at all is a transport error.
    pub(super) async fn send(
        &self,
        action: &'static str,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ForgeError> {
        let response = request
            .send()
            .await
            .map_err(|source| ForgeError::Transport {
                action,
                source: Box::new(source),
            })?;
        answer(action, response).await
    }

    /// The answer's body as `T`.
    pub(super) async fn read<T: DeserializeOwned>(
        &self,
        action: &'static str,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ForgeError> {
        self.send(action, request)
            .await?
            .json()
            .await
            .map_err(|source| ForgeError::Response {
                action,
                source: Box::new(source),
            })
    }

    /// Every item of a paginated list, read page after page.
    async fn read_pages<T: DeserializeOwned>(
        &self,
        action: &'static str,
        path: &str,
    ) -> Result<Vec<T>, ForgeError> {
        let mut items = Vec::new();
        for page in 1.. {
            let page_items: Vec<T> = self
                .read(
                    action,
                    self.request(
                        reqwest::Method::GET,
                        &format!("{path}?per_page={PAGE_SIZE}&page={page}"),
                    ),
                )
                .await?;
            let last = page_items.len() < PAGE_SIZE;
            items.extend(page_items);
            if last {
                break;
            }
        }
        Ok(items)
    }

    /// The open pull request on `branch` carrying this run's marker —
    /// the one a resumed gate keeps using, with the title and description
    /// it holds. A PR someone closed or merged is never picked up again.
    pub(super) async fn find_open_pr(
        &self,
        branch: &str,
        run_id: &str,
    ) -> Result<Option<OpenPr>, ForgeError> {
        #[derive(Deserialize)]
        struct PrSummary {
            number: u64,
            html_url: String,
            title: String,
            #[serde(default)]
            body: Option<String>,
        }
        let prs: Vec<PrSummary> = self
            .read(
                "find the open pull request",
                self.request(
                    reqwest::Method::GET,
                    &self.repo_path(&format!(
                        "/pulls?head={}:{branch}&state=open",
                        self.repo.owner()
                    )),
                ),
            )
            .await?;
        Ok(prs
            .into_iter()
            .find(|pr| {
                pr.body
                    .as_deref()
                    .is_some_and(|body| super::marked(body, run_id))
            })
            .map(|pr| OpenPr {
                reference: PullRequestRef {
                    url: pr.html_url,
                    number: pr.number,
                },
                title: pr.title,
                body: pr.body.unwrap_or_default(),
            }))
    }

    async fn review_comments(&self, number: u64) -> Result<Vec<ReviewComment>, ForgeError> {
        #[derive(Deserialize)]
        struct Comment {
            user: CommentUser,
            body: String,
            path: Option<String>,
        }
        #[derive(Deserialize)]
        struct CommentUser {
            login: String,
        }
        let comments: Vec<Comment> = self
            .read_pages(
                "read the review comments",
                &self.repo_path(&format!("/pulls/{number}/comments")),
            )
            .await?;
        Ok(comments
            .into_iter()
            .map(|c| ReviewComment {
                author: c.user.login,
                body: c.body,
                path: c.path,
            })
            .collect())
    }
}

/// A success as it is; a rate limit as [`ForgeError::RateLimited`];
/// any other refusal as [`ForgeError::Http`] with its body.
async fn answer(
    action: &'static str,
    response: reqwest::Response,
) -> Result<reqwest::Response, ForgeError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    if let Some(limit) = rate_limit(status.as_u16(), response.headers()) {
        return Err(ForgeError::RateLimited {
            action,
            retry_after: limit.retry_after,
        });
    }
    let body = response.text().await.unwrap_or_default();
    Err(ForgeError::Http {
        action,
        status: status.as_u16(),
        body,
    })
}

struct RateLimit {
    retry_after: Option<Duration>,
}

/// GitHub refuses a spent rate limit with 403 or 429, `x-ratelimit-
/// remaining: 0` and either a `retry-after` in seconds or an
/// `x-ratelimit-reset` epoch; the wait is whichever it named.
fn rate_limit(status: u16, headers: &HeaderMap) -> Option<RateLimit> {
    if status != 403 && status != 429 {
        return None;
    }
    let spent = header(headers, "x-ratelimit-remaining").as_deref() == Some("0");
    let retry_after = header(headers, "retry-after")
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs);
    if !spent && retry_after.is_none() {
        return None;
    }
    let reset = header(headers, "x-ratelimit-reset")
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|epoch| {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some(Duration::from_secs(epoch.saturating_sub(now)))
        });
    Some(RateLimit {
        retry_after: retry_after.or(reset),
    })
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_string)
}

#[async_trait::async_trait]
impl Forge for GitHubForge {
    async fn publish(&self, req: &PublishRequest) -> Result<PullRequestRef, ForgeError> {
        let run_id = req.run_id.as_str();
        // Drawn before anything is read or committed, so a description the forge
        // would refuse leaves the branch as it was.
        let body = super::gate_body(req)?;
        let existing = self.find_open_pr(&req.branch, run_id).await?;
        if existing.is_none() {
            let base_sha = self.base_branch_sha(&req.base_branch).await?;
            self.ensure_branch(&req.branch, &base_sha).await?;
        }
        // Every lap commits what it decides on, so a pull request reused
        // after a request for changes shows the correction.
        for (path, content) in super::gate_files(req) {
            let message =
                CommitMessage::of_published(&req.run_id, &req.decision.node, &path).text();
            self.commit_artifact(&path, &content, &req.branch, &message)
                .await?;
        }
        let title = super::gate_title(req);
        match existing {
            Some(open) => {
                // A reused pull request shows this lap: its title and description
                // are redrawn only when one of them differs from what the forge holds.
                if open.title != title || !super::same_text(&open.body, &body) {
                    self.update_pr(open.reference.number, &title, &body).await?;
                }
                Ok(open.reference)
            }
            None => {
                self.create_pr(&title, &req.branch, &req.base_branch, &body)
                    .await
            }
        }
    }

    async fn open_pull_request(
        &self,
        req: &PullRequestRequest,
    ) -> Result<PullRequestRef, ForgeError> {
        self.open(req).await
    }

    async fn probe(&self) -> Result<ForgeProbe, ForgeError> {
        self.repository().await
    }

    async fn poll(&self, gate: &PullRequestRef) -> Result<PolledGate, ForgeError> {
        self.poll_gate(gate).await
    }
}

/// A commit id the API reported, or the answer refused for not being one.
fn parse_sha(action: &'static str, sha: &str) -> Result<CommitSha, ForgeError> {
    sha.parse()
        .map_err(|source| ForgeError::Malformed { action, source })
}

/// A reviewer's login the API reported, or the answer refused for an
/// empty one.
fn parse_responder(action: &'static str, login: &str) -> Result<Responder, ForgeError> {
    login
        .parse()
        .map_err(|source| ForgeError::Malformed { action, source })
}
