//! The pull requests a `GitHubForge` opens for a branch a run already
//! pushed, and what it says about the token it holds.

use serde::Deserialize;
use yunta_core::port::{ForgeError, ForgeProbe, PullRequestRef, PullRequestRequest};

use super::github::{run_marker, GitHubForge};

impl GitHubForge {
    /// The open pull request this run's marker names on `head`, or a new
    /// one: its body is the caller's, then the marker.
    pub(super) async fn open(
        &self,
        req: &PullRequestRequest,
    ) -> Result<PullRequestRef, ForgeError> {
        if let Some(existing) = self.find_open_pr(&req.head, &req.run_id).await? {
            return Ok(existing);
        }
        let body = format!("{}\n\n---\n{}", req.body, run_marker(&req.run_id));
        self.create_pr(&req.title, &req.head, &req.base, &body)
            .await
    }

    /// `POST /pulls`: a pull request of `head` into `base`.
    pub(super) async fn create_pr(
        &self,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
    ) -> Result<PullRequestRef, ForgeError> {
        #[derive(Deserialize)]
        struct CreatedPr {
            number: u64,
            html_url: String,
        }
        let request = self
            .request(reqwest::Method::POST, &self.repo_path("/pulls"))
            .json(&serde_json::json!({
                "title": title,
                "head": head,
                "base": base,
                "body": body,
            }));
        let created: CreatedPr = self.read("open the pull request", request).await?;
        Ok(PullRequestRef {
            url: created.html_url,
            number: created.number,
        })
    }

    /// `GET /repos/{owner}/{name}`: whether the repository answers this
    /// token, and whether the token may push there.
    pub(super) async fn repository(&self) -> Result<ForgeProbe, ForgeError> {
        #[derive(Deserialize)]
        struct Repository {
            #[serde(default)]
            permissions: Option<Permissions>,
        }
        #[derive(Deserialize)]
        struct Permissions {
            #[serde(default)]
            push: bool,
        }
        let repository: Repository = self
            .read(
                "read the repository",
                self.request(reqwest::Method::GET, &self.repo_path("")),
            )
            .await?;
        Ok(ForgeProbe {
            can_push: repository.permissions.map(|permissions| permissions.push),
        })
    }
}
