//! Reading a gate's pull request back from GitHub: its head, and the last
//! decisive review, which decides what the run does next.

use serde::Deserialize;
use yunta_core::port::{ForgeError, PolledGate, PullRequestRef, ReviewOutcome};
use yunta_core::{CommitSha, Responder};

use super::{parse_responder, parse_sha, GitHubForge};

/// The pull request as GitHub returns it: whether it is open, and how it ended.
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

/// One review of the pull request, in the order GitHub gives them.
#[derive(Deserialize)]
struct Review {
    state: String,
    user: User,
    commit_id: Option<String>,
}

impl GitHubForge {
    /// The gate's pull request as the forge holds it, with the review
    /// that decides it if one has been given.
    pub(super) async fn poll_gate(&self, gate: &PullRequestRef) -> Result<PolledGate, ForgeError> {
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
            let review = ended(detail, &head_sha)?;
            return Ok(PolledGate { head_sha, review });
        }

        let reviews: Vec<Review> = self
            .read_pages(
                "read the reviews",
                &self.repo_path(&format!("/pulls/{}/reviews", gate.number)),
            )
            .await?;
        let review = self.decision(gate, reviews).await?;
        Ok(PolledGate { head_sha, review })
    }

    /// The decision the reviews of an open pull request give.
    async fn decision(
        &self,
        gate: &PullRequestRef,
        reviews: Vec<Review>,
    ) -> Result<ReviewOutcome, ForgeError> {
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
        Ok(review)
    }
}

/// The outcome of a closed pull request: merged, by whom and
/// at which commit, or closed without a merge.
fn ended(detail: PrDetail, head_sha: &CommitSha) -> Result<ReviewOutcome, ForgeError> {
    if !detail.merged {
        return Ok(ReviewOutcome::Closed);
    }
    Ok(ReviewOutcome::Merged {
        by: match detail.merged_by {
            Some(user) => parse_responder("read the pull request", &user.login)?,
            None => Responder::from_static("(unknown)"),
        },
        merge_sha: match detail.merge_commit_sha {
            Some(sha) => parse_sha("read the pull request", &sha)?,
            None => head_sha.clone(),
        },
    })
}
