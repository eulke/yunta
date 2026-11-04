//! A gate published to a forge, as its pull request says it: the title it
//! is known by, the description a reviewer reads within what the forge keeps
//! of one, and the files the pull request carries beside it.
//!
//! The description is the gate's page, in the forge's Markdown: a reviewer
//! decides on what the pull request says, and a second arrangement of one page
//! would drift from the first. Its sections are headings of the second level
//! under the forge's title, a card and the proof are folded, the risks are an
//! alert, the steps' order is Mermaid, and the decision is a table of the answers
//! a review gives, the commands of the machine that holds the run after it. No
//! answer names a command of its own: a published gate is answered on its forge,
//! and the one command that moves the run after any answer is said once. It is
//! not cut at the limit, which would cut the decision, its last part, or break a
//! fence or a table: it gives things up whole, code first.

use surco::components::{Chosen, Decision, DecisionOption, Section};
use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::line::{Line, Tone};
use surco::surface::{Description, Leaves, Markdown, Surface};
use yunta_core::port::{GateDecision, PublishRequest};
use yunta_core::shown::{ShownContent, ShownDocument};
use yunta_core::step::NextStep;

use crate::gate::GatePage;
use crate::plan::Form;

/// The forge a gate's pull request is drawn for, and what it keeps.
pub struct Publication<'a> {
    /// What the forge is called where a reviewer answers on it: `GitHub`.
    pub forge: &'a str,
    /// The most characters the forge keeps in a pull request's description. A
    /// description is measured against it in UTF-8 bytes, which are never
    /// fewer than its characters, so one within it in bytes is kept whole.
    pub body_limit: usize,
    /// What writes each command the page names.
    pub spell: &'a dyn Fn(&NextStep) -> String,
}

/// The title of `request`'s pull request: `Approve:` and the change the plan
/// shown says it makes, or else the gate's question. A title longer than
/// `title_limit` characters, which is how the forge counts them, is cut to its
/// first `title_limit - 1` characters and ends in an ellipsis, so it is
/// `title_limit` characters long. `title_limit` is at least 1, the room an
/// ellipsis takes.
pub fn title(request: &PublishRequest, title_limit: usize) -> String {
    debug_assert!(title_limit >= 1, "a cut title keeps its ellipsis");
    let title = match yunta_core::shown::change_shown(&request.shown) {
        Some(summary) => format!("Approve: {}", crate::claim::change(summary)),
        None => crate::claim::sentence(&request.decision.question, None),
    };
    if title.chars().count() <= title_limit {
        return title;
    }
    let kept: String = title.chars().take(title_limit - 1).collect();
    format!("{kept}{}", Glyphs::Unicode.ellipsis())
}

/// The description of `request`'s pull request: the gate's page as a reviewer
/// reads it, then `tail` after a blank line, within what the forge keeps. Its
/// length is counted in UTF-8 bytes, which are never fewer than the characters
/// the forge counts, so a description within the limit in bytes is within it
/// in characters too. Below the least limit, which is the decision with its
/// question cut to an ellipsis, not even that fits: the description is drawn
/// over the limit, and the caller must refuse it rather than publish it.
pub fn body(request: &PublishRequest, publication: &Publication<'_>, tail: &str) -> String {
    let room = publication
        .body_limit
        .saturating_sub(tail.len() + BLANK_LINE.len());
    format!("{}{BLANK_LINE}{tail}", fitted(request, publication, room))
}

/// Every file the pull request carries, at its path in the gate's folder:
/// each shown document drawn whole as Markdown under the name it is published
/// by, then each artifact's bytes under its own name.
pub fn files(
    request: &PublishRequest,
    spell: &dyn Fn(&NextStep) -> String,
) -> Vec<(String, Vec<u8>)> {
    let drawn = request.shown.iter().filter_map(|document| {
        let name = drawn_name(document)?;
        let doc = crate::shown::document(document, &request.run_id, Form::Whole, spell);
        Some((request.folder.at(&name), Markdown.draw(&doc).into_bytes()))
    });
    let held = request
        .artifacts
        .iter()
        .map(|(name, bytes)| (request.folder.at(name), bytes.clone()));
    drawn.chain(held).collect()
}

/// What sets the tail off from the description: a blank line, which a
/// forge's Markdown keeps as the break between two paragraphs.
const BLANK_LINE: &str = "\n\n";

/// How much of the gate's page a description draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    /// The documents, with what `Leaves` names left out of each.
    Documents(Leaves),
    /// The documents with what `Leaves::Folds` leaves out, and each text document down to
    /// its title: it has no lines of code or folds to give up.
    TextTitled,
    /// None of the documents: the headline, where the decision waits, the note,
    /// what the run changed and the decision the page ends on.
    NoDocuments,
    /// The decision alone: the headline, where it waits, the note, what the run
    /// changed, and the decision under its question with the files and each
    /// answer, without what weighs on it.
    Decision,
}

/// The levels a description is drawn at, from the whole page to the decision
/// alone. Each leaves out more than the one before it, and the decision is in
/// every one: each code block's lines, what each fold holds, a text document's
/// words but its title, every document, and then what weighs on the decision.
/// Code goes first: it is the bulk of a plan, and it reads as well in the
/// forge's file viewer. The first that fits is drawn.
const LEVELS: [Level; 6] = [
    Level::Documents(Leaves::Nothing),
    Level::Documents(Leaves::CodeLines),
    Level::Documents(Leaves::Folds),
    Level::TextTitled,
    Level::NoDocuments,
    Level::Decision,
];

/// The description at the first level whose text fits in `room` bytes. The
/// decision is in every level, so when even the decision alone is too long,
/// which only a question of tens of thousands of characters makes it, the
/// question is cut to fit. A room too small even for the decision with its
/// question cut to an ellipsis keeps none of the question, and the decision is
/// drawn over it: `body` then returns more than its limit.
fn fitted(request: &PublishRequest, publication: &Publication<'_>, room: usize) -> String {
    let question = crate::claim::sentence(&request.decision.question, None);
    for level in LEVELS {
        let drawn = draw(request, publication, level, &question);
        if drawn.len() <= room {
            return drawn;
        }
    }
    let bare = draw(request, publication, Level::Decision, "").len();
    let kept = cut_question(&question, room.saturating_sub(bare));
    draw(request, publication, Level::Decision, &kept)
}

/// `question` cut to a start and an ellipsis that fit in `left` bytes, the room
/// the decision alone leaves for the question. The start keeps at most half of
/// what is left after the ellipsis, cut at a character boundary: an escape at
/// most doubles a sign, so the start still fits in that room once it is drawn.
/// Where `left` is shorter than the ellipsis, the start is empty and not even
/// the ellipsis fits.
fn cut_question(question: &str, left: usize) -> String {
    let ellipsis = Glyphs::Unicode.ellipsis();
    let mut end = (left.saturating_sub(ellipsis.len()) / 2).min(question.len());
    while !question.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ellipsis}", question[..end].trim_end())
}

/// The gate's page at `level`, drawn as a description: each document in the
/// form the page shows, what the level leaves out said under the gist, and the
/// decision the page ends on, titled by `question`. The text has no trailing
/// space.
fn draw(
    request: &PublishRequest,
    publication: &Publication<'_>,
    level: Level,
    question: &str,
) -> String {
    let titled: Vec<ShownDocument>;
    let (documents, leaves): (&[ShownDocument], Leaves) = match level {
        Level::Documents(leaves) => (request.shown.as_slice(), leaves),
        Level::TextTitled => {
            titled = request.shown.iter().map(down_to_its_title).collect();
            (titled.as_slice(), Leaves::Folds)
        }
        Level::NoDocuments | Level::Decision => (&[], Leaves::Nothing),
    };
    let page = crate::gate::page(
        GatePage {
            shown: Some((&request.run_id, documents)),
            waits: Some(format!(
                "Gate `{}` waits on this pull request, for {}.",
                request.decision.node, request.decision.assignee
            )),
            note: note(publication, level),
            changed: request
                .changed
                .as_ref()
                .and_then(|change| crate::changes::of(change, Glyphs::Unicode)),
            form: Form::Published,
            decision: decision(request, publication, question, level),
        },
        publication.spell,
    );
    Description { leaves }.draw(&page).trim_end().to_string()
}

/// `document` with a text document's words down to the heading it opens on: its title on the
/// page says what it is, and the file the pull request carries holds it whole.
fn down_to_its_title(document: &ShownDocument) -> ShownDocument {
    let mut document = document.clone();
    if let ShownContent::Text(text) = &mut document.content {
        *text = crate::shown::heading_line(text);
    }
    document
}

/// What `level` leaves out, and where it is whole, as the note the page says
/// under its gist. `None` where the level leaves nothing out.
fn note(publication: &Publication<'_>, level: Level) -> Option<String> {
    let left = match level {
        Level::Documents(Leaves::Nothing) => return None,
        Level::Documents(Leaves::CodeLines) => "the lines of each code block",
        Level::Documents(Leaves::Folds) => {
            "the lines of each code block and what each folded section holds"
        }
        Level::TextTitled => {
            "the lines of each code block, what each folded section holds and what each text \
             document says"
        }
        Level::NoDocuments => "the documents the gate shows",
        Level::Decision => "the documents the gate shows and what weighs on the decision",
    };
    Some(format!(
        "This description leaves out {left}, {}; the files it carries hold every document \
         whole.",
        why_it_leaves_out(publication.forge, publication.body_limit)
    ))
}

/// Why a description leaves something out: what `forge` keeps of one.
pub fn why_it_leaves_out(forge: &str, limit: usize) -> String {
    format!("since {forge} keeps a pull request's description to {limit} characters")
}

/// The section the page ends on, titled by `question`: the files it is read
/// in, what weighs on the decision unless `level` is the decision alone, each
/// answer with what it does, and the commands of the machine that holds the run.
fn decision(
    request: &PublishRequest,
    publication: &Publication<'_>,
    question: &str,
    level: Level,
) -> Section {
    let mut blocks = vec![Block::Lines(listed(request))];
    let weighing = match level {
        Level::Decision => None,
        Level::Documents(_) | Level::TextTitled | Level::NoDocuments => {
            crate::shown::before_you_decide(&[], &request.shown)
        }
    };
    blocks.extend(weighing.map(Block::Section));
    blocks.push(Block::Decision(Decision {
        chosen: Chosen::OnTheForge {
            forge: publication.forge.to_string(),
            elsewhere: ELSEWHERE.to_string(),
        },
        options: answers(&request.decision),
        otherwise: vec![
            (
                (publication.spell)(&NextStep::Resume {
                    run: request.run_id.clone(),
                }),
                "reads the answer and acts on it",
            ),
            (
                (publication.spell)(&NextStep::Close {
                    run: request.run_id.clone(),
                }),
                "closes the run for good",
            ),
        ],
    }));
    crate::gate::decision(question, blocks)
}

/// Where what follows a review's options is typed: on the machine that holds
/// the run, never on the forge.
const ELSEWHERE: &str = "on the machine that holds the run";

/// One line per file the pull request carries: its path in the gate's folder,
/// then what a reader finds in it. Documents come first, then each artifact as
/// the run holds it.
fn listed(request: &PublishRequest) -> Vec<Line> {
    let drawn = request.shown.iter().filter_map(|document| {
        Some((
            request.folder.at(&drawn_name(document)?),
            read(&document.content)?,
        ))
    });
    let held = request
        .artifacts
        .iter()
        .map(|(name, _)| (request.folder.at(name), "as the run holds it".to_string()));
    drawn
        .chain(held)
        .map(|(path, what)| {
            Line::new()
                .push(Tone::Command, path)
                .plain(": ")
                .plain(what)
        })
        .collect()
}

/// What a review on the forge can answer, and what each answer does to the run.
fn answers(decision: &GateDecision) -> Vec<DecisionOption> {
    let changes = match &decision.corrected_by {
        Some(node) => format!(
            "each comment reaches `{node}` as a finding; it runs again, what it makes \
             lands on this pull request, and the gate asks again here"
        ),
        None => "each comment becomes a finding, and the gate fails".to_string(),
    };
    vec![
        DecisionOption {
            id: "approve".to_string(),
            label: Some("approve this pull request, or merge it".to_string()),
            tradeoff: Some(crate::offers::resolving(&decision.then)),
            asks: None,
        },
        DecisionOption {
            id: "request changes".to_string(),
            label: Some("request changes, saying what to change in comments".to_string()),
            tradeoff: Some(changes),
            asks: None,
        },
        DecisionOption {
            id: "close".to_string(),
            label: Some("close this pull request".to_string()),
            tradeoff: Some(crate::offers::STAYS_OPEN.to_string()),
            asks: None,
        },
    ]
}

/// The name `document` is published by drawn, when it is drawn: a
/// document the run reads into its parts is; text is its own bytes.
fn drawn_name(document: &ShownDocument) -> Option<String> {
    match &document.content {
        ShownContent::Text(_) => None,
        _ => document.shown.artifact.drawn_name(),
    }
}

/// What a reader finds in a document drawn to read.
fn read(content: &ShownContent) -> Option<String> {
    Some(
        match content {
            ShownContent::Tasks(review) if review.spec.is_some() => {
                "the whole plan, each task with its code and each test the spec wrote"
            }
            ShownContent::Tasks(_) => "the whole plan, each task with its code and what proves it",
            ShownContent::Spec(_) => "the tests the plan's tasks are held to",
            ShownContent::Findings(_) => "what the review found, the most severe first",
            ShownContent::RunFindings(_) => "every finding the run holds",
            ShownContent::Text(_) => return None,
        }
        .to_string(),
    )
}
