//! What a round of questions says: its question, what it says under it, what an
//! optional question says beside it, the names a boolean is picked by, what stands
//! for an answer not given, and the keys the round answers to.

use surco::components::{Card, CardBody, Legend, RoundRow};
use surco::text::counted;
use yunta_core::{NodeId, Question};

use super::{list_legend, Escape, ESC};

/// The option that leaves a question no answer, offered only where the question allows
/// one, and what stands for the answer that was not given when the choice is echoed back.
pub const SKIP: &str = "(no answer)";

/// What a round of more than one question says under its question: leaving it with
/// Escape records nothing of it.
pub const NOTHING_KEPT: &str = "nothing is recorded until every one is answered";

/// The round of `rows` the node `node` asks, as its card: how many answers it needs
/// before the node goes on, that nothing is kept of it until the last when it asks more
/// than one, every question in its place, and `legend`.
pub fn card(node: &NodeId, rows: Vec<RoundRow>, legend: Legend) -> Card {
    let total = rows.len();
    Card {
        question: format!("{} before `{node}` goes on", counted(total, "answer")),
        edge: None,
        about: match total > 1 {
            true => vec![NOTHING_KEPT.to_string()],
            false => Vec::new(),
        },
        body: CardBody::Round(rows),
        legend,
    }
}

/// The keys a round answers to: the list's own when a list is being answered, enter,
/// going back once there is a question before, and what Escape does.
pub fn legend(picking: bool, goes_back: bool, escape: Escape) -> Legend {
    let legend = match picking {
        true => list_legend(),
        false => Legend::new(),
    }
    .key("enter", "answers");
    let legend = match goes_back {
        true => legend.key("shift-tab", "goes back"),
        false => legend,
    };
    legend.key(ESC, escape.does())
}

/// What a question says beside it when it may be left: that it is optional, and what
/// is assumed without an answer.
pub fn note(question: &Question) -> Option<String> {
    if question.required {
        return None;
    }
    Some(match &question.assumes {
        Some(assumes) => format!("optional, assumes {assumes}"),
        None => "optional".to_string(),
    })
}

/// The name one of the two answers a `boolean` question takes is picked and read by.
pub fn boolean(value: bool) -> &'static str {
    match value {
        true => "yes",
        false => "no",
    }
}
