//! `GitHubForge`: the only real `Forge` in v1. Talks to GitHub's REST
//! API directly (no local git) — the Contents/Refs endpoints can create
//! a branch and commit files over plain HTTP, so publishing a gate
//! needs nothing beyond the same bearer token that reads its PR state
//! back.
//!
//! **No live smoke test against the real API — a documented gap, not a
//! silent skip**, the same shape the `codex` adapter already documents
//! for the same reason: no GitHub token or a disposable repo to publish
//! real PRs against exists in this environment. Every endpoint/field
//! used here is documented, current GitHub REST API v3 shape
//! (`git/refs`, `contents`, `pulls`, `pulls/.../reviews`,
//! `pulls/.../comments`), not a guess — but that's not the same as
//! having run it.

use base64::Engine;
use serde::Deserialize;

use super::{
    Forge, ForgeError, PolledGate, PublishRequest, PublishedGate, ReviewComment, ReviewOutcome,
};

const API_VERSION: &str = "2022-11-28";

pub struct GitHubForge {
    client: reqwest::Client,
    /// `owner/repo`.
    repo: String,
    token: String,
    /// Overridable for tests that want to point at a local stub server
    /// without touching `api.github.com` — defaults to the real API.
    base_url: String,
}

impl GitHubForge {
    pub fn new(repo: String, token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            repo,
            token,
            base_url: "https://api.github.com".to_string(),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", "yunta")
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
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{}/git/ref/heads/{base_branch}", self.repo),
            )
            .send()
            .await
            .map_err(|e| ForgeError::Request(e.to_string()))?;
        let resp = ensure_ok(resp).await?;
        let parsed: RefObject = resp
            .json()
            .await
            .map_err(|e| ForgeError::UnexpectedResponse(e.to_string()))?;
        Ok(parsed.object.sha)
    }

    /// Idempotent: GitHub answers 422 when the ref already exists, which
    /// is exactly the "already published, `resume` is calling this
    /// again" case — not an error here.
    async fn ensure_branch(&self, branch: &str, base_sha: &str) -> Result<(), ForgeError> {
        let resp = self
            .request(
                reqwest::Method::POST,
                &format!("/repos/{}/git/refs", self.repo),
            )
            .json(&serde_json::json!({
                "ref": format!("refs/heads/{branch}"),
                "sha": base_sha,
            }))
            .send()
            .await
            .map_err(|e| ForgeError::Request(e.to_string()))?;
        if resp.status().as_u16() == 422 {
            return Ok(()); // already exists
        }
        ensure_ok(resp).await?;
        Ok(())
    }

    /// GitHub's Contents API needs the existing blob's `sha` to update a
    /// file that's already there (409 without it) but must omit it to
    /// create a new one — so this looks the file up first.
    async fn existing_file_sha(&self, path: &str, branch: &str) -> Option<String> {
        #[derive(Deserialize)]
        struct ContentsMeta {
            sha: String,
        }
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{}/contents/{path}?ref={branch}", self.repo),
            )
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<ContentsMeta>().await.ok().map(|m| m.sha)
    }

    async fn commit_artifact(
        &self,
        path: &str,
        content: &[u8],
        branch: &str,
        message: &str,
    ) -> Result<(), ForgeError> {
        let existing_sha = self.existing_file_sha(path, branch).await;
        let mut body = serde_json::json!({
            "message": message,
            "content": base64::engine::general_purpose::STANDARD.encode(content),
            "branch": branch,
        });
        if let Some(sha) = existing_sha {
            body["sha"] = serde_json::Value::String(sha);
        }
        let resp = self
            .request(
                reqwest::Method::PUT,
                &format!("/repos/{}/contents/{path}", self.repo),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| ForgeError::Request(e.to_string()))?;
        ensure_ok(resp).await?;
        Ok(())
    }

    async fn find_open_pr(&self, branch: &str) -> Result<Option<PublishedGate>, ForgeError> {
        #[derive(Deserialize)]
        struct PrSummary {
            number: u64,
            html_url: String,
        }
        let owner = self.repo.split('/').next().unwrap_or_default();
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{}/pulls?head={owner}:{branch}&state=all", self.repo),
            )
            .send()
            .await
            .map_err(|e| ForgeError::Request(e.to_string()))?;
        let resp = ensure_ok(resp).await?;
        let prs: Vec<PrSummary> = resp
            .json()
            .await
            .map_err(|e| ForgeError::UnexpectedResponse(e.to_string()))?;
        Ok(prs.into_iter().next().map(|pr| PublishedGate {
            url: pr.html_url,
            number: pr.number,
        }))
    }
}

async fn ensure_ok(resp: reqwest::Response) -> Result<reqwest::Response, ForgeError> {
    if resp.status().is_success() {
        Ok(resp)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(ForgeError::Request(format!("{status}: {body}")))
    }
}

#[async_trait::async_trait]
impl Forge for GitHubForge {
    async fn publish(&self, req: &PublishRequest) -> Result<PublishedGate, ForgeError> {
        if let Some(existing) = self.find_open_pr(&req.branch).await? {
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
        let resp = self
            .request(
                reqwest::Method::POST,
                &format!("/repos/{}/pulls", self.repo),
            )
            .json(&serde_json::json!({
                "title": format!("yunta: {}", req.summary),
                "head": req.branch,
                "base": req.base_branch,
                "body": format!(
                    "{}\n\n---\nrun_id: `{}`\n\n_Opened by Yunta — approve or request \
                     changes like any other PR review._",
                    req.summary, req.run_id
                ),
            }))
            .send()
            .await
            .map_err(|e| ForgeError::Request(e.to_string()))?;
        let resp = ensure_ok(resp).await?;
        let created: CreatedPr = resp
            .json()
            .await
            .map_err(|e| ForgeError::UnexpectedResponse(e.to_string()))?;
        Ok(PublishedGate {
            url: created.html_url,
            number: created.number,
        })
    }

    async fn poll(&self, gate: &PublishedGate) -> Result<PolledGate, ForgeError> {
        #[derive(Deserialize)]
        struct PrDetail {
            state: String,
            head: PrHead,
        }
        #[derive(Deserialize)]
        struct PrHead {
            sha: String,
        }
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{}/pulls/{}", self.repo, gate.number),
            )
            .send()
            .await
            .map_err(|e| ForgeError::Request(e.to_string()))?;
        let resp = ensure_ok(resp).await?;
        let detail: PrDetail = resp
            .json()
            .await
            .map_err(|e| ForgeError::UnexpectedResponse(e.to_string()))?;

        if detail.state != "open" {
            return Ok(PolledGate {
                head_sha: detail.head.sha,
                review: ReviewOutcome::Closed,
            });
        }

        #[derive(Deserialize)]
        struct Review {
            state: String,
            user: User,
            commit_id: Option<String>,
        }
        #[derive(Deserialize)]
        struct User {
            login: String,
        }
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{}/pulls/{}/reviews", self.repo, gate.number),
            )
            .send()
            .await
            .map_err(|e| ForgeError::Request(e.to_string()))?;
        let resp = ensure_ok(resp).await?;
        let reviews: Vec<Review> = resp
            .json()
            .await
            .map_err(|e| ForgeError::UnexpectedResponse(e.to_string()))?;

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

impl GitHubForge {
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
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{}/pulls/{}/comments", self.repo, number),
            )
            .send()
            .await
            .map_err(|e| ForgeError::Request(e.to_string()))?;
        let resp = ensure_ok(resp).await?;
        let comments: Vec<Comment> = resp
            .json()
            .await
            .map_err(|e| ForgeError::UnexpectedResponse(e.to_string()))?;
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
