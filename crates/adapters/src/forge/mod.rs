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

use surco::glyphs::Glyphs;
use surco::surface::{Description, Surface};
use yunta_core::port::{ForgeError, PublishRequest, PullRequestRequest};
use yunta_core::step::NextStep;
use yunta_render::published::{self, Publication};

/// What a reviewer answers a gate on.
const FORGE: &str = "GitHub";

/// The most characters a pull request's title keeps on GitHub, which counts a title in
/// characters, as its API does, not in display cells: a combining mark takes no cell and
/// is still a character. GitHub refuses an issue title past 256 characters, and a pull
/// request is an issue there.
const TITLE_LIMIT: usize = 256;

/// The most characters GitHub keeps in a pull request's description, as its refusal
/// states it. GitHub documents no count, so a description is measured against it in
/// UTF-8 bytes, which are never fewer than its characters.
const BODY_LIMIT: usize = 65_536;

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
/// then the run's receipt drawn as a description, then the marker. `spell`
/// writes the next steps the receipt names.
pub(crate) fn pull_request_body(
    req: &PullRequestRequest,
    spell: &dyn Fn(&NextStep) -> String,
) -> String {
    let receipt = req.receipt.as_ref().map(|receipt| {
        // A description, not a page: the forge's title already heads the pull
        // request, so the receipt draws no title of its own. Its separators are
        // the Unicode set whatever look the terminal chose.
        Description::default().draw(&yunta_render::receipt::document(
            receipt,
            spell,
            Glyphs::Unicode,
        ))
    });
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

/// The forge a gate's pull request is drawn for, and the limit its description keeps.
fn publication(spell: &dyn Fn(&NextStep) -> String) -> Publication<'_> {
    Publication {
        forge: FORGE,
        body_limit: BODY_LIMIT,
        spell,
    }
}

/// The title a gate's pull request opens with and is redrawn with: the change
/// the plan shown says it makes, or else the question the gate asks.
pub(crate) fn gate_title(req: &PublishRequest) -> String {
    published::title(req, TITLE_LIMIT)
}

/// The body a gate's pull request opens with and is redrawn with: how a review
/// answers the gate and what each answer does, then the run's marker. A body
/// drawn longer than the forge keeps is refused, so the forge is never asked
/// for it.
pub(crate) fn gate_body(
    req: &PublishRequest,
    spell: &dyn Fn(&NextStep) -> String,
) -> Result<String, ForgeError> {
    let page = publication(spell);
    let body = published::body(req, &page, &run_marker(req.run_id.as_str()));
    match body.len() > page.body_limit {
        true => Err(ForgeError::BodyTooLong {
            limit: page.body_limit,
            len: body.len(),
        }),
        false => Ok(body),
    }
}

/// Every file a gate's pull request carries, each at its path in the gate's folder.
pub(crate) fn gate_files(
    req: &PublishRequest,
    spell: &dyn Fn(&NextStep) -> String,
) -> Vec<(String, Vec<u8>)> {
    published::files(req, spell)
}

/// Whether two drawn texts are the same text. A forge may return a description
/// with CRLF line endings, so every carriage return before a line feed is dropped
/// before the texts are compared.
pub(super) fn same_text(a: &str, b: &str) -> bool {
    a.replace("\r\n", "\n") == b.replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::{gate_title, same_text};
    use std::path::PathBuf;
    use yunta_core::events::{ArtifactId, Claim, GateFolder, Shown};
    use yunta_core::port::{GateDecision, PublishRequest};
    use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
    use yunta_core::{ArtifactKind, ContentHash, TasksFile};

    /// A gate's request for `question`, with no document shown beside it.
    fn request(question: Claim) -> PublishRequest {
        PublishRequest {
            branch: "yunta/run-1/gate".to_string(),
            base_branch: "main".to_string(),
            run_id: "run-1".parse().unwrap(),
            folder: GateFolder::Root,
            decision: GateDecision {
                node: "review".into(),
                question,
                assignee: "lead".to_string(),
                then: Vec::new(),
                corrected_by: None,
            },
            artifacts: Vec::new(),
            shown: Vec::new(),
            changed: None,
        }
    }

    /// A plan shown beside a gate, whose summary is `summary`.
    fn shown_plan(summary: &str) -> ShownDocument {
        ShownDocument {
            shown: Shown {
                producer: None,
                artifact: ArtifactId::Interpreted {
                    kind: ArtifactKind::Tasks,
                },
                content_hash: ContentHash::sha256(b"plan"),
            },
            path: PathBuf::from("artifacts/plan/tasks.yaml"),
            content: ShownContent::Tasks(Box::new(PlanReview {
                plan: TasksFile {
                    summary: Some(summary.to_string()),
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

    /// A gate's pull request is titled with the claim it makes: the author's
    /// message as written, or that the gate needs a decision.
    #[test]
    fn a_gate_titles_its_pull_request_with_the_claim_it_makes() {
        assert_eq!(
            gate_title(&request(Claim::Written("Is the spec ready?".to_string()))),
            "Is the spec ready?"
        );
        assert_eq!(
            gate_title(&request(Claim::NeedsDecision {
                node: "review".into(),
            })),
            "gate `review` needs a decision"
        );
    }

    /// A shown plan titles the pull request with the change its summary says,
    /// ahead of the gate's own question.
    #[test]
    fn a_shown_plan_titles_its_pull_request_with_its_change() {
        let mut gate = request(Claim::Written("Is the spec ready?".to_string()));
        gate.shown = vec![shown_plan("Make it")];

        assert_eq!(gate_title(&gate), "Approve: Make it");
    }

    /// A description a forge returns with CRLF line endings is the same text as
    /// its LF form, and a changed line is a different text.
    #[test]
    fn a_crlf_description_is_the_same_text_as_its_lf_form() {
        assert!(same_text("## a\r\n\r\nbody\r\n", "## a\n\nbody\n"));
        assert!(!same_text("## a\n\nbody\n", "## a\n\nother\n"));
    }
}
