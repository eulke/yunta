//! What a round of questions says: its question, what it says under it, what an
//! optional question says beside it, the names a boolean is picked by, what stands
//! for an answer not given, the request a round in place stands under, and the keys the
//! round answers to.

use surco::components::{Card, CardBody, Legend, RoundRow};
use surco::doc::Block;
use surco::line::{Line, Tone};
use yunta_core::{NodeId, Question};

use super::{list_legend, PromptAct, ENTER, ESC};

/// The option that leaves a question no answer, offered only where the question allows
/// one, and what stands for the answer that was not given when the choice is echoed back.
pub const SKIP: &str = "(no answer)";

/// What a round of more than one question says under its question, and the gist of a run
/// it holds says of it: leaving it with Escape records nothing of it.
pub const NOTHING_KEPT: &str = "nothing is recorded until every one is answered";

/// The round of `rows` the node `node` asks, as its card: that the node asks, that nothing
/// is kept of it until the last when it asks more than one, every question in its place,
/// and `legend`. Its questions say how many it asks.
pub fn card(node: &NodeId, rows: Vec<RoundRow>, legend: Legend) -> Card {
    let total = rows.len();
    Card {
        question: format!("{node} asks"),
        edge: None,
        about: match total > 1 {
            true => vec![NOTHING_KEPT.to_string()],
            false => Vec::new(),
        },
        body: CardBody::Round(rows),
        legend,
    }
}

/// The row a round in place stands under: what the person asked the run for, `request`,
/// after the words that say so, muted.
pub fn you_asked(request: &str) -> Block {
    let row = Line::new()
        .push(Tone::Muted, "you asked")
        .plain("  ")
        .push(Tone::Plain, request);
    Block::Rows(vec![row])
}

/// The keys a round answers to, on a run's screen and at a terminal alike: the list's own
/// when a list is being answered, enter, going back once there is a question before, and
/// `escape`, what Escape does there.
pub fn legend(picking: bool, goes_back: bool, escape: &str) -> Legend {
    let legend = match picking {
        true => list_legend(),
        false => Legend::new(),
    }
    .key(ENTER, PromptAct::Answer.said());
    let legend = match goes_back {
        true => legend.key("shift-tab", PromptAct::GoBack.said()),
        false => legend,
    };
    legend.key(ESC, escape)
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
