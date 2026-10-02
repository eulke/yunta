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
//! that way; that is the manual smoke test's job.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use yunta_core::{CommitSha, GitHubRepo, Responder, Secret};

use yunta_core::port::{
    Forge, ForgeError, ForgeProbe, PolledGate, PublishRequest, PullRequestRef, PullRequestRequest,
    ReviewComment, ReviewOutcome,
};

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

impl GitHubForge {
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
    async fn send(
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

    async fn base_branch_sha(&self, base_branch: &str) -> Result<String, ForgeError> {
        #[derive(Deserialize)]
        struct RefObject {
            object: RefSha,
        }
        #[derive(Deserialize)]
        struct RefSha {
            sha: String,
        }
        let parsed: RefObject = self
            .read(
                "resolve the base branch",
                self.request(
                    reqwest::Method::GET,
                    &self.repo_path(&format!("/git/ref/heads/{base_branch}")),
                ),
            )
            .await?;
        Ok(parsed.object.sha)
    }

    /// Idempotent: GitHub answers 422 when the ref already exists, which
    /// is exactly the "already published, `resume` is calling this
    /// again" case — not an error here.
    async fn ensure_branch(&self, branch: &str, base_sha: &str) -> Result<(), ForgeError> {
        let request = self
            .request(reqwest::Method::POST, &self.repo_path("/git/refs"))
            .json(&serde_json::json!({
                "ref": format!("refs/heads/{branch}"),
                "sha": base_sha,
            }));
        match self.send("create the branch", request).await {
            Ok(_) | Err(ForgeError::Http { status: 422, .. }) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// GitHub's Contents API needs the existing blob's `sha` to update a
    /// file that's already there (409 without it) but must omit it to
    /// create a new one — so this looks the file up first. Only a 404
    /// means "no such file"; any other refusal is reported as it is.
    async fn existing_file(
        &self,
        path: &str,
        branch: &str,
    ) -> Result<Option<ExistingFile>, ForgeError> {
        #[derive(Deserialize)]
        struct Contents {
            sha: String,
            #[serde(default)]
            encoding: Option<String>,
            #[serde(default)]
            content: Option<String>,
        }
        let action = "read an artifact's current version";
        let request = self.request(
            reqwest::Method::GET,
            &self.repo_path(&format!("/contents/{path}?ref={branch}")),
        );
        match self.send(action, request).await {
            Ok(response) => {
                let found: Contents =
                    response
                        .json()
                        .await
                        .map_err(|source| ForgeError::Response {
                            action,
                            source: Box::new(source),
                        })?;
                // A file too large to come inline says `none`: its bytes
                // are unknown, never taken for empty.
                let content = match (found.encoding.as_deref(), found.content) {
                    (Some("base64"), Some(encoded)) => {
                        let packed: String =
                            encoded.chars().filter(|c| !c.is_whitespace()).collect();
                        base64::engine::general_purpose::STANDARD
                            .decode(packed)
                            .ok()
                    }
                    _ => None,
                };
                Ok(Some(ExistingFile {
                    sha: found.sha,
                    content,
                }))
            }
            Err(ForgeError::Http { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Commits `content` to `path` on `branch` — unless the file already
    /// holds it: a commit that changes nothing would move the branch's
    /// head, and a review of it would no longer cover what it reviewed.
    async fn commit_artifact(
        &self,
        path: &str,
        content: &[u8],
        branch: &str,
        message: &str,
    ) -> Result<(), ForgeError> {
        let existing = self.existing_file(path, branch).await?;
        if existing
            .as_ref()
            .is_some_and(|file| file.content.as_deref() == Some(content))
        {
            return Ok(());
        }
        let mut body = serde_json::json!({
            "message": message,
            "content": base64::engine::general_purpose::STANDARD.encode(content),
            "branch": branch,
        });
        if let (Some(file), Some(object)) = (existing, body.as_object_mut()) {
            object.insert("sha".to_string(), serde_json::Value::String(file.sha));
        }
        let request = self
            .request(
                reqwest::Method::PUT,
                &self.repo_path(&format!("/contents/{path}")),
            )
            .json(&body);
        self.send("commit an artifact", request).await?;
        Ok(())
    }

    /// The open pull request on `branch` carrying this run's marker —
    /// the one a resumed gate keeps using. A PR someone closed or
    /// merged is never picked up again.
    pub(super) async fn find_open_pr(
        &self,
        branch: &str,
        run_id: &str,
    ) -> Result<Option<PullRequestRef>, ForgeError> {
        #[derive(Deserialize)]
        struct PrSummary {
            number: u64,
            html_url: String,
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
            .map(|pr| PullRequestRef {
                url: pr.html_url,
                number: pr.number,
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

/// A file a branch already holds: the blob id an update names, and its
/// bytes when the forge sent them inline.
struct ExistingFile {
    sha: String,
    content: Option<Vec<u8>>,
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_string)
}

#[async_trait::async_trait]
impl Forge for GitHubForge {
    async fn publish(&self, req: &PublishRequest) -> Result<PullRequestRef, ForgeError> {
        let run_id = req.run_id.as_str();
        let existing = self.find_open_pr(&req.branch, run_id).await?;
        if existing.is_none() {
            let base_sha = self.base_branch_sha(&req.base_branch).await?;
            self.ensure_branch(&req.branch, &base_sha).await?;
        }
        // Every lap commits what it decides on, so a pull request reused
        // after a request for changes shows the correction.
        let message = format!(
            "publish what gate `{}` of run {run_id} decides on",
            req.decision.node
        );
        for (path, content) in super::gate_files(req) {
            self.commit_artifact(&path, &content, &req.branch, &message)
                .await?;
        }
        match existing {
            Some(existing) => Ok(existing),
            None => {
                self.create_pr(
                    &super::gate_title(req),
                    &req.branch,
                    &req.base_branch,
                    &super::gate_body(req),
                )
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
        #[derive(Deserialize)]
        struct PrDetail {
            state: String,
            #[serde(default)]
            merged: bool,
            merged_by: Option<User>,
            merge_commit_sha: Option<String>,
            head: PrHead,
        }
        #[derive(Deserialize)]
        struct PrHead {
            sha: String,
        }
        #[derive(Deserialize)]
        struct User {
            login: String,
        }
        let detail: PrDetail = self
            .read(
                "read the pull request",
                self.request(
                    reqwest::Method::GET,
                    &self.repo_path(&format!("/pulls/{}", gate.number)),
                ),
            )
            .await?;

        let head_sha = parse_sha("read the pull request", &detail.head.sha)?;
        if detail.state != "open" {
            let review = if detail.merged {
                ReviewOutcome::Merged {
                    by: match detail.merged_by {
                        Some(user) => parse_responder("read the pull request", &user.login)?,
                        None => Responder::from_static("(unknown)"),
                    },
                    merge_sha: match detail.merge_commit_sha {
                        Some(sha) => parse_sha("read the pull request", &sha)?,
                        None => head_sha.clone(),
                    },
                }
            } else {
                ReviewOutcome::Closed
            };
            return Ok(PolledGate { head_sha, review });
        }

        #[derive(Deserialize)]
        struct Review {
            state: String,
            user: User,
            commit_id: Option<String>,
        }
        let reviews: Vec<Review> = self
            .read_pages(
                "read the reviews",
                &self.repo_path(&format!("/pulls/{}/reviews", gate.number)),
            )
            .await?;

        // Last decisive review wins (APPROVED/CHANGES_REQUESTED) —
        // COMMENTED/DISMISSED aren't decisions this maps to anything.
        // GitHub returns reviews in submission order, oldest first.
        let last_decision = reviews
            .into_iter()
            .rfind(|r| r.state == "APPROVED" || r.state == "CHANGES_REQUESTED");

        let review = match last_decision {
            None => ReviewOutcome::Pending,
            Some(r) => match r.commit_id.as_deref().map(str::parse::<CommitSha>) {
                Some(Ok(reviewed_sha)) if r.state == "APPROVED" => ReviewOutcome::Approved {
                    by: parse_responder("read the pull request reviews", &r.user.login)?,
                    reviewed_sha,
                },
                Some(Ok(reviewed_sha)) => {
                    let comments = self.review_comments(gate.number).await?;
                    ReviewOutcome::ChangesRequested {
                        by: parse_responder("read the pull request reviews", &r.user.login)?,
                        reviewed_sha,
                        comments,
                    }
                }
                // A review that names no commit covers no code: it decides
                // nothing, so the gate keeps waiting and the log says why.
                _ => {
                    tracing::warn!(
                        review_state = %r.state,
                        commit_id = ?r.commit_id,
                        "a review without a commit id is not a decision; the gate keeps waiting"
                    );
                    ReviewOutcome::Pending
                }
            },
        };

        Ok(PolledGate { head_sha, review })
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
