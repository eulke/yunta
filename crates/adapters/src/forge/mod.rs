//! The two forges this workspace builds, behind `yunta_core::port::Forge`.
//!
//! `GitHubForge` talks to GitHub's REST API directly — no local git
//! needed, since the Contents and Refs endpoints create a branch and
//! commit files over HTTP alone. `MockForge` is the fixture-driven
//! double the engine's own gate tests run against: the same "never a
//! real network call in CI" stance taken for LLM adapters, extended to
//! forges.

mod github;
mod github_pulls;
mod mock;

pub use github::GitHubForge;
pub use mock::{MockForge, MockForgeState, MockPullRequest};

use yunta_core::port::{PublishRequest, PullRequestRequest};
use yunta_render::surface::{Markdown, Surface};

/// What ties a pull request's body to its run: a comment a forge's page
/// does not show, so the marker never sits in what a reviewer reads.
pub(crate) fn run_marker(run_id: &str) -> String {
    format!("<!-- yunta run_id: {run_id} -->")
}

/// The marker a pull request opened before the comment form carries,
/// which a run that opened it still finds it by.
pub(crate) fn earlier_run_marker(run_id: &str) -> String {
    format!("run_id: `{run_id}`")
}

/// Whether `body` belongs to the run `run_id`, by either marker.
pub(crate) fn marked(body: &str, run_id: &str) -> bool {
    body.contains(&run_marker(run_id)) || body.contains(&earlier_run_marker(run_id))
}

/// The body a run's pull request opens with: the body its author wrote,
/// then the run's receipt drawn as Markdown, then the marker.
pub(crate) fn pull_request_body(req: &PullRequestRequest) -> String {
    let receipt = req
        .receipt
        .as_ref()
        .map(|receipt| Markdown.draw(&yunta_render::receipt::document(receipt)));
    let parts: Vec<String> = [Some(req.body.trim().to_string()), receipt]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .map(|part| part.trim_end().to_string())
        .collect();
    let mut body = parts.join("\n\n---\n\n");
    if !body.is_empty() {
        body.push_str("\n\n");
    }
    body.push_str(&run_marker(&req.run_id));
    body
}

/// The title a gate's pull request opens with: the question it asks.
pub(crate) fn gate_title(req: &PublishRequest) -> String {
    req.decision.question.clone()
}

/// The body a gate's pull request opens with: how a review answers it
/// and what each answer does to the run, then the marker.
pub(crate) fn gate_body(req: &PublishRequest) -> String {
    let drawn = Markdown.draw(&yunta_render::published::gate(req));
    format!(
        "{}\n\n{}",
        drawn.trim_end(),
        run_marker(req.run_id.as_str())
    )
}

/// Every file a gate's pull request carries: each document a person
/// reads drawn to read, then each artifact as the run holds it.
pub(crate) fn gate_files(req: &PublishRequest) -> Vec<(String, Vec<u8>)> {
    yunta_render::published::drawn(req)
        .into_iter()
        .map(|(path, drawn)| (path, drawn.into_bytes()))
        .chain(req.artifacts.iter().cloned())
        .collect()
}
