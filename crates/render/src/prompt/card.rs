//! What a decision's card says: what stands on its far edge and what a reader needs
//! under its question; the keys it answers to with its list open or with words being
//! typed; the words being said for the option chosen, with what is asked of them and
//! why the ones given were not taken; and the line a choice is kept as.

use surco::components::{CardBody, Choices, Legend};
use surco::glyphs::Glyphs;
use yunta_core::events::{ChangesCountedPayload, Claim, GateWaitingPayload};
use yunta_core::shown::ShownDocument;
use yunta_core::{NodeId, OptionId, RunId};

use super::{typed_row, PromptAct, ENTER, ESC};
use crate::page::desk;

/// The keys a card's options are chosen with: `j k` on a run's screen, whose arrows move
/// what stands above the card, and the arrows at a terminal, where nothing does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChosenWith {
    Letters,
    Arrows,
}

/// What an option that needs words says when none were given.
pub const NEEDS_WORDS: &str = "this option needs an answer";

/// What a note is asked with, on an option that asks for no words of its own.
pub const A_NOTE: &str = "add a note";

/// What stands on a card's far edge: the `node` the decision is on, or the handle of the
/// `run` it stopped where it is on no node.
pub fn edge(node: Option<&NodeId>, run: Option<&RunId>) -> Option<String> {
    node.map(ToString::to_string)
        .or_else(|| run.map(|run| run.handle().to_string()))
}

/// What a reader needs to answer `escalation`, under its question: why a task asked for
/// the scope the question names, when it asks at the engine's terminal; what the plan it
/// is on says about the question, when it is on one among `documents`; the record the
/// engine attached to it, audited against those documents; then what the run `changed`,
/// when the change was counted.
pub fn about(
    escalation: &GateWaitingPayload,
    documents: &[ShownDocument],
    changed: Option<&ChangesCountedPayload>,
    glyphs: Glyphs,
) -> Vec<String> {
    let question = crate::escalation::question(escalation);
    let mut about: Vec<String> = match escalation.summary() {
        Claim::ScopeAsked { .. } => crate::escalation::why_it_asks(escalation)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    };
    about.extend(
        desk::plan_among(documents)
            .map(|(_, review)| desk::about(review, &question, glyphs))
            .unwrap_or_default(),
    );
    about.extend(crate::escalation::record(escalation, documents));
    about.extend(changed.and_then(|change| crate::changes::of(change, glyphs)));
    about
}

/// The line a choice is kept as in the history: the `option`, and the `words` it was
/// said with, as the person wrote them.
pub fn chose(option: &OptionId, words: Option<&str>) -> String {
    match words {
        Some(words) => format!("chose `{option}`: {words}"),
        None => format!("chose `{option}`"),
    }
}

/// The keys a card answers to with its list open, on a run's screen and at a terminal
/// alike: choosing with the keys `chosen` says, answering, a note, reading the plan the
/// decision is on when the card `reads` one, and `escape`, what Escape does there.
pub fn choosing_legend(chosen: ChosenWith, reads: bool, escape: &str) -> Legend {
    let choose = PromptAct::Choose.said();
    let legend = match chosen {
        ChosenWith::Letters => Legend::new().key("j k", choose),
        ChosenWith::Arrows => Legend::new().arrows(choose),
    }
    .key(ENTER, PromptAct::Answer.said())
    .key("n", PromptAct::AddANote.said());
    let legend = match reads {
        true => legend.key("r", PromptAct::ReadThePlan.said()),
        false => legend,
    };
    legend.key(ESC, escape)
}

/// The keys a card choosing its option takes that its legend leaves to help, where help over a
/// screen holding it says them: `tab` and `shift-tab`, and the number of each of its
/// `options`, choose as `j k` do.
pub fn left_to_help(options: usize) -> Legend {
    let choose = PromptAct::Choose.said();
    Legend::new()
        .key("tab shift-tab", choose)
        .key(super::numbered(options), choose)
}

/// The keys while words are typed for an option, on a card or at a plan's desk: sending
/// them where the option `asks` for words, recording them as a note where it does not,
/// and going back.
pub fn saying_legend(asks: bool) -> Legend {
    let send = match asks {
        true => PromptAct::SendIt,
        false => PromptAct::RecordIt,
    };
    Legend::new()
        .key(ENTER, send.said())
        .key(ESC, PromptAct::GoBack.said())
}

/// The words being said for the option `choices` points at, every option still in sight:
/// what it asks of them, or a note where it asks for none; the row they are typed on,
/// before the cursor and after it; and, where an answer was `refused` for having none, that
/// the option needs one.
pub fn saying(
    choices: Choices,
    asks: Option<String>,
    (before, after): (String, String),
    refused: bool,
) -> CardBody {
    CardBody::Saying {
        choices,
        asks: asks.unwrap_or_else(|| A_NOTE.to_string()),
        typed: typed_row(before, after),
        refused: refused.then(|| NEEDS_WORDS.to_string()),
    }
}
