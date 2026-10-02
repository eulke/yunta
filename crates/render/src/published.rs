//! A gate published to a forge, as its pull request says it: who it
//! waits on, how a review answers it and what each answer does to the
//! run, what was published for it, and what the machine that holds the
//! run is told to do next.

use yunta_core::events::ArtifactId;
use yunta_core::port::{GateDecision, PublishRequest};
use yunta_core::shown::{ShownContent, ShownDocument};
use yunta_core::NodeId;

use crate::blocks::{Chosen, Decision, DecisionOption, Next, Prose};
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
use crate::plan::Form;
use crate::surface::{Markdown, Surface};

/// What the pull request of `request` says under its title, which is the
/// gate's question.
pub fn gate(request: &PublishRequest) -> Doc<'static> {
    let decision = &request.decision;
    let handle = request.run_id.handle();
    let published = files(request)
        .into_iter()
        .map(|(path, what)| {
            Line::new()
                .push(Tone::Command, path)
                .plain(" — ")
                .plain(what)
        })
        .collect();
    Doc::new()
        .with(Prose(format!(
            "Gate `{}` of run `{}` waits on this pull request, for {}.",
            decision.node, request.run_id, decision.assignee
        )))
        .with(Block::Heading("how to answer".to_string()))
        .with(Decision {
            chosen: Chosen::OnTheForge,
            options: answers(decision),
        })
        .with(Block::Heading("what it decides on".to_string()))
        .with(Block::Lines(published))
        .with(Block::Heading(
            "on the machine that holds the run".to_string(),
        ))
        .with(Next {
            steps: vec![
                (
                    format!("yunta resume {handle}"),
                    "reads the review and goes on from it",
                ),
                (format!("yunta cancel {handle}"), "stops the run instead"),
            ],
        })
}

/// Each document of `request` a person reads drawn whole, as Markdown,
/// under the name it is published by beside the artifact it draws:
/// `tasks.yaml` is read as `tasks.md`.
pub fn drawn(request: &PublishRequest) -> Vec<(String, String)> {
    let handle = request.run_id.handle();
    request
        .shown
        .iter()
        .filter_map(|document| {
            let name = drawn_name(document)?;
            let doc = crate::shown::document(document, handle, Form::Whole);
            Some((name, Markdown.draw(&doc)))
        })
        .collect()
}

/// The name `document` is published by drawn, when it is drawn: a
/// document the run reads into its parts is; text is its own bytes.
fn drawn_name(document: &ShownDocument) -> Option<String> {
    match (&document.shown.artifact, &document.content) {
        (_, ShownContent::Text(_)) => None,
        (ArtifactId::Interpreted { kind }, _) => Some(format!("{kind}.md")),
        (ArtifactId::Opaque { .. }, _) => None,
    }
}

/// Every file the pull request carries, with what a reader finds in it.
fn files(request: &PublishRequest) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = request
        .shown
        .iter()
        .filter_map(|document| Some((drawn_name(document)?, read(&document.content)?)))
        .collect();
    files.extend(
        request
            .artifacts
            .iter()
            .map(|(path, _)| (path.clone(), "as the run holds it".to_string())),
    );
    files
}

/// What a reader finds in a document drawn to read.
fn read(content: &ShownContent) -> Option<String> {
    Some(
        match content {
            ShownContent::Tasks(review) if review.spec.is_some() => {
                "the plan, task by task, with the tests that hold each one"
            }
            ShownContent::Tasks(_) => "the plan, task by task, with what proves each one",
            ShownContent::Spec(_) => "the tests the plan's tasks are held to",
            ShownContent::Findings(_) => "what the review found, the most severe first",
            ShownContent::RunFindings(_) => "every finding the run holds",
            ShownContent::Text(_) => return None,
        }
        .to_string(),
    )
}

/// What a review on the forge can say, and what each does to the run.
fn answers(decision: &GateDecision) -> Vec<DecisionOption> {
    let goes_on = match decision.then.as_slice() {
        [] => "the gate passes".to_string(),
        next => format!("the gate passes, and the run goes on to {}", named(next)),
    };
    let corrected = match &decision.corrected_by {
        Some(node) => format!(
            "each comment reaches `{node}` as a finding; it runs again, what it makes \
             lands on this pull request, and the gate asks again here"
        ),
        None => STOPS.to_string(),
    };
    vec![
        DecisionOption {
            id: "approve".to_string(),
            label: Some("approve this pull request, or merge it".to_string()),
            tradeoff: goes_on,
            asks: None,
        },
        DecisionOption {
            id: "request changes".to_string(),
            label: Some("request changes, saying what to change in comments".to_string()),
            tradeoff: corrected,
            asks: None,
        },
        DecisionOption {
            id: "close".to_string(),
            label: Some("close this pull request".to_string()),
            tradeoff: STOPS.to_string(),
            asks: None,
        },
    ]
}

/// What an answer that fails the gate does to the run.
const STOPS: &str = "the gate fails, and the run goes no further past it";

/// `nodes` in backticks, joined as a sentence names them.
fn named(nodes: &[NodeId]) -> String {
    let named: Vec<String> = nodes.iter().map(|node| format!("`{node}`")).collect();
    match named.as_slice() {
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        [] => String::new(),
    }
}
