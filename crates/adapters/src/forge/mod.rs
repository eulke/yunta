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
use yunta_core::RunId;
use yunta_render::page::{self, Page};
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

/// The body a run's pull request opens with: the body its author wrote, then the run's
/// receipt drawn as a description, then the marker, within what the forge keeps. A receipt
/// that does not fit is left out, and an author's words that do not fit even alone are cut:
/// a note says so, and the marker is never what gives.
pub(crate) fn pull_request_body(req: &PullRequestRequest) -> String {
    let receipt = req.receipt.as_ref().map(|receipt| {
        // A description, not a page: the forge's title already heads the pull
        // request, so the receipt draws no title of its own, in the forge's look
        // whatever look the terminal chose.
        let description = Description::default();
        description.draw(&page::Receipt { receipt }.document(&description.look()))
    });
    let author = req.body.trim();
    let marker = run_marker(&req.run_id);
    let whole = body_of(&[author, receipt.as_deref().unwrap_or_default()], &marker);
    if whole.len() <= BODY_LIMIT {
        return whole;
    }
    let command = receipt.as_ref().and_then(|_| receipt_command(&req.run_id));
    if receipt.is_some() {
        let note = left_out("the run's receipt", command.as_deref());
        let kept = body_of(&[author, note.as_str()], &marker);
        if kept.len() <= BODY_LIMIT {
            return kept;
        }
    }
    let left = match receipt.is_some() {
        true => "the run's receipt and the end of what this node says",
        false => "the end of what this node says",
    };
    let note = left_out(left, command.as_deref());
    let tail = body_of(&["", note.as_str()], &marker);
    let room = BODY_LIMIT.saturating_sub(tail.len() + SEPARATOR.len());
    let cut = cut_to(author, room);
    body_of(&[cut.as_str(), note.as_str()], &marker)
}

/// What sets the parts of a body apart: a rule, which a forge's Markdown draws as a break
/// between two sections.
const SEPARATOR: &str = "\n\n---\n\n";

/// `parts` that say anything, set apart by a rule, then `marker`.
fn body_of(parts: &[&str], marker: &str) -> String {
    let said: Vec<&str> = parts
        .iter()
        .map(|part| part.trim_end())
        .filter(|part| !part.trim().is_empty())
        .collect();
    match said.is_empty() {
        true => marker.to_string(),
        false => format!("{}\n\n{marker}", said.join(SEPARATOR)),
    }
}

/// The command that prints the run's receipt; `None` when `run_id` does not parse as a
/// run id, and the note then names the receipt without a command.
fn receipt_command(run_id: &str) -> Option<String> {
    let run = run_id.parse::<RunId>().ok()?;
    Some(yunta_render::command::line(&NextStep::Receipt { run }))
}

/// What a body leaves out and where it is whole, said where a reviewer reads it. `receipt`
/// is the command that prints the run's receipt, when a receipt was drawn.
fn left_out(left: &str, receipt: Option<&str>) -> String {
    let whole = match receipt {
        Some(command) => format!("; `{command}` prints the run's receipt"),
        None => String::new(),
    };
    format!(
        "This description leaves out {left}, {}{whole}.",
        published::why_it_leaves_out(FORGE, BODY_LIMIT)
    )
}

/// `author` cut to fit `room` bytes: the characters that fit with an ellipsis after them,
/// trimmed at their end. The author is never empty where this is reached, since a body
/// whose receipt is left out and whose words are empty already fits.
fn cut_to(author: &str, room: usize) -> String {
    let ellipsis = Glyphs::Unicode.ellipsis();
    let keep = room.saturating_sub(ellipsis.len());
    let mut kept = String::new();
    for ch in author.chars() {
        if kept.len() + ch.len_utf8() > keep {
            break;
        }
        kept.push(ch);
    }
    format!("{}{ellipsis}", kept.trim_end())
}

/// The forge a gate's pull request is drawn for, and the limit its description keeps.
const PUBLICATION: Publication<'static> = Publication {
    forge: FORGE,
    body_limit: BODY_LIMIT,
};

/// The title a gate's pull request opens with and is redrawn with: the change
/// the plan shown says it makes, or else the question the gate asks.
pub(crate) fn gate_title(req: &PublishRequest) -> String {
    published::title(req, TITLE_LIMIT)
}

/// The body a gate's pull request opens with and is redrawn with: how a review
/// answers the gate and what each answer does, then the run's marker. A body
/// drawn longer than the forge keeps is refused, so the forge is never asked
/// for it.
pub(crate) fn gate_body(req: &PublishRequest) -> Result<String, ForgeError> {
    let publication = PUBLICATION;
    let body = published::body(req, &publication, &run_marker(req.run_id.as_str()));
    match body.len() > publication.body_limit {
        true => Err(ForgeError::BodyTooLong {
            limit: publication.body_limit,
            len: body.len(),
        }),
        false => Ok(body),
    }
}

/// Every file a gate's pull request carries, each at its path in the gate's folder.
pub(crate) fn gate_files(req: &PublishRequest) -> Vec<(String, Vec<u8>)> {
    published::files(req)
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
