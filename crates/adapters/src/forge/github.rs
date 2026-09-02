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
use yunta_core::{GitHubRepo, Secret};

use super::{
    Forge, ForgeError, PolledGate, PublishRequest, PublishedGate, ReviewComment, ReviewOutcome,
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
                source,
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

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(self.token.expose())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", "yunta")
    }

    /// `/repos/{owner}/{name}{rest}`.
    fn repo_path(&self, rest: &str) -> String {
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
            .map_err(|source| ForgeError::Transport { action, source })?;
        answer(action, response).await
    }

    /// The answer's body as `T`.
    async fn read<T: DeserializeOwned>(
        &self,
        action: &'static str,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ForgeError> {
        self.send(action, request)
            .await?
            .json()
            .await
            .map_err(|source| ForgeError::Response { action, source })
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
    async fn existing_file_sha(
        &self,
        path: &str,
        branch: &str,
    ) -> Result<Option<String>, ForgeError> {
        #[derive(Deserialize)]
        struct ContentsMeta {
            sha: String,
        }
        let action = "read an artifact's current version";
        let request = self.request(
            reqwest::Method::GET,
            &self.repo_path(&format!("/contents/{path}?ref={branch}")),
        );
        match self.send(action, request).await {
            Ok(response) => {
                let meta: ContentsMeta = response
                    .json()
                    .await
                    .map_err(|source| ForgeError::Response { action, source })?;
                Ok(Some(meta.sha))
            }
            Err(ForgeError::Http { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn commit_artifact(
        &self,
        path: &str,
        content: &[u8],
        branch: &str,
        message: &str,
    ) -> Result<(), ForgeError> {
        let existing_sha = self.existing_file_sha(path, branch).await?;
        let mut body = serde_json::json!({
            "message": message,
            "content": base64::engine::general_purpose::STANDARD.encode(content),
            "branch": branch,
        });
        if let Some(sha) = existing_sha {
            body["sha"] = serde_json::Value::String(sha);
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
    async fn find_open_pr(
        &self,
        branch: &str,
        run_id: &str,
    ) -> Result<Option<PublishedGate>, ForgeError> {
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
        let marker = run_marker(run_id);
        Ok(prs
            .into_iter()
            .find(|pr| {
                pr.body
                    .as_deref()
                    .is_some_and(|body| body.contains(&marker))
            })
            .map(|pr| PublishedGate {
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

/// The line in a PR body that ties it to its run.
fn run_marker(run_id: &str) -> String {
    format!("run_id: `{run_id}`")
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
    async fn publish(&self, req: &PublishRequest) -> Result<PublishedGate, ForgeError> {
        if let Some(existing) = self.find_open_pr(&req.branch, &req.run_id).await? {
            return Ok(existing);
        }

        let base_sha = self.base_branch_sha(&req.base_branch).await?;
        self.ensure_branch(&req.branch, &base_sha).await?;
        for (path, content) in &req.artifacts {
            self.commit_artifact(
                path,
                content,
                &req.branch,
                &format!("yunta: publish gate artifacts for run {}", req.run_id),
            )
            .await?;
        }

        #[derive(Deserialize)]
        struct CreatedPr {
            number: u64,
            html_url: String,
        }
        let request = self
            .request(reqwest::Method::POST, &self.repo_path("/pulls"))
            .json(&serde_json::json!({
                "title": format!("yunta: {}", req.summary),
                "head": req.branch,
                "base": req.base_branch,
                "body": format!(
                    "{}\n\n---\n{}\n\n_Opened by Yunta — approve or request \
                     changes like any other PR review._",
                    req.summary,
                    run_marker(&req.run_id)
                ),
            }));
        let created: CreatedPr = self.read("open the pull request", request).await?;
        Ok(PublishedGate {
            url: created.html_url,
            number: created.number,
        })
    }

    async fn poll(&self, gate: &PublishedGate) -> Result<PolledGate, ForgeError> {
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

        if detail.state != "open" {
            let review = if detail.merged {
                ReviewOutcome::Merged {
                    by: detail
                        .merged_by
                        .map(|user| user.login)
                        .unwrap_or_else(|| "(unknown)".to_string()),
                    merge_sha: detail
                        .merge_commit_sha
                        .unwrap_or_else(|| detail.head.sha.clone()),
                }
            } else {
                ReviewOutcome::Closed
            };
            return Ok(PolledGate {
                head_sha: detail.head.sha,
                review,
            });
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
            Some(r) if r.state == "APPROVED" => ReviewOutcome::Approved {
                by: r.user.login,
                reviewed_sha: r.commit_id.unwrap_or_default(),
            },
            Some(r) => {
                let comments = self.review_comments(gate.number).await?;
                ReviewOutcome::ChangesRequested {
                    by: r.user.login,
                    reviewed_sha: r.commit_id.unwrap_or_default(),
                    comments,
                }
            }
        };

        Ok(PolledGate {
            head_sha: detail.head.sha,
            review,
        })
    }
}
