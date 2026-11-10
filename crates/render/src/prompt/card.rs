//! What a decision's card says: what stands on its far edge and what a reader needs
//! under its question; the keys it answers to with its list open or with words being
//! typed; the words being said for the option chosen, with what is asked of them and
//! why the ones given were not taken; and the line a choice is kept as.

use surco::components::{CardBody, ChoiceRow, Legend, TypedRow};
use surco::glyphs::Glyphs;
use yunta_core::events::{ChangesCountedPayload, GateWaitingPayload};
use yunta_core::shown::ShownDocument;
use yunta_core::{NodeId, OptionId, RunId};

use super::{Escape, ANSWER, ESC};
use crate::page::desk;

/// What an option that needs words says when none were given.
pub const NEEDS_WORDS: &str = "this option needs an answer";

/// What a note is asked with, on an option that asks for no words of its own.
pub const A_NOTE: &str = "add a note";

/// What stands on a card's far edge: the `node` the decision is on and the handle of the
/// `run` it stopped, apart by a separator, whichever of them there are.
pub fn edge(node: Option<&NodeId>, run: Option<&RunId>, glyphs: Glyphs) -> Option<String> {
    let parts: Vec<String> = node
        .map(ToString::to_string)
        .into_iter()
        .chain(run.map(|run| run.handle().to_string()))
        .collect();
    let separator = format!(" {} ", glyphs.sep());
    (!parts.is_empty()).then(|| parts.join(separator.as_str()))
}

/// What a reader needs to answer `escalation`, under its question: what the plan it is
/// on says about the question, when it is on one among `documents`; the record the
/// engine attached to it, audited against those documents; then what the run `changed`,
/// when the change was counted.
pub fn about(
    escalation: &GateWaitingPayload,
    documents: &[ShownDocument],
    changed: Option<&ChangesCountedPayload>,
    glyphs: Glyphs,
) -> Vec<String> {
    let question = crate::escalation::claim_words(escalation);
    let mut about: Vec<String> = desk::plan_among(documents)
        .map(|(_, review)| desk::about(review, &question, glyphs))
        .unwrap_or_default();
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

/// The keys a card answers to with its list open: the arrows, choosing, a note, reading
/// the plan the decision is on when the card `reads` one, and what Escape does.
pub fn choosing_legend(reads: bool, escape: Escape) -> Legend {
    let legend = Legend::new()
        .arrows("move")
        .key("enter", "chooses")
        .key("n", "adds a note");
    let legend = match reads {
        true => legend.key("r", "reads the plan"),
        false => legend,
    };
    legend.key(ESC, escape.does())
}

/// The keys a card answers to while words are typed for an option: sending them where
/// the option `asks` for words, recording them as a note where it does not, and going
/// back.
pub fn saying_legend(asks: bool) -> Legend {
    let send = match asks {
        true => "sends it",
        false => "records it",
    };
    Legend::new().key("enter", send).key(ESC, "goes back")
}

/// The words being said for `chosen`: what it asks of them, or a note where it asks
/// for none; the row they are typed on, before the cursor and after it; and, where an
/// answer was `refused` for having none, that the option needs one.
pub fn saying(
    chosen: ChoiceRow,
    asks: Option<String>,
    (before, after): (String, String),
    refused: bool,
) -> CardBody {
    CardBody::Saying {
        chosen,
        asks: asks.unwrap_or_else(|| A_NOTE.to_string()),
        typed: TypedRow {
            prompt: ANSWER.to_string(),
            before,
            after,
        },
        refused: refused.then(|| NEEDS_WORDS.to_string()),
    }
}
