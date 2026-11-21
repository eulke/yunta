//! What a plan's desk says around what is read: its title, what stands at its far
//! edge, the keys that do something while it is read, on a run's screen and at a terminal
//! alike, and the words being typed at its foot for an option that asks for them.

use surco::components::{ChoiceRow, FootRow, Legend};
use surco::words::Standing;

use super::card::NEEDS_WORDS;
use super::{numbered, typed_row, PromptAct, ESC};
use crate::screen::BY_THE_PAGE;

/// What `tab` does at the plan's desk, and `j k` alike: goes from place to place.
pub const NEXT_PLACE: &str = "next place";

/// What `shift-tab` does at the plan's desk: goes back to the place before.
pub const PLACE_BEFORE: &str = "place before";

/// What `p` does at the plan's desk: turns the place stood on over to its proof, or back.
pub const PROOF: &str = "proof";

/// What `d` does at the plan's desk: shows what the plan decided for the person, or the
/// place again.
pub const DECIDED_FOR_YOU: &str = "decided for you";

/// The title a desk is headed with: the plan, then the run's handle and the workflow it
/// follows, by its name, each when there is one.
pub fn title(run: Option<String>, workflow: Option<String>) -> Vec<String> {
    std::iter::once("plan".to_string())
        .chain(run)
        .chain(workflow)
        .collect()
}

/// What stands on the desk's far edge: what needs the person, and the node it is on.
pub fn edge(node: Option<&str>) -> String {
    let needs = Standing::NeedsYou.word();
    match node {
        Some(node) => format!("{needs}  {node}"),
        None => needs.to_string(),
    }
}

/// The words being said for `chosen` at the desk's foot, in place of its options: what it
/// `asks` of them; the row they are typed on, before the cursor and after it; and, where an
/// answer was `refused` for having none, that the option needs one, as a card says it.
pub fn saying(
    chosen: ChoiceRow,
    asks: String,
    (before, after): (String, String),
    refused: bool,
) -> FootRow {
    FootRow::Saying {
        chosen,
        asks,
        typed: typed_row(before, after),
        refused: refused.then(|| NEEDS_WORDS.to_string()),
    }
}

/// The keys of a plan's desk while it is read: a number for each of its `options` (up to
/// `9`), the next place, what is read, the proof where the place has one, what is decided
/// where the plan says it, and `escape`, what Escape does where the desk stands.
pub fn legend(options: usize, has_proof: bool, says_decided: bool, escape: &str) -> Legend {
    let legend = Legend::new()
        .key(numbered(options), PromptAct::Answer.said())
        .key("tab", NEXT_PLACE)
        .arrows("scroll");
    let legend = match has_proof {
        true => legend.key("p", PROOF),
        false => legend,
    };
    let legend = match says_decided {
        true => legend.key("d", DECIDED_FOR_YOU),
        false => legend,
    };
    legend.key(ESC, escape)
}

/// The keys the desk takes that its legend leaves to help, where help over a screen holding
/// it says them: `j k` to the next place as `tab` goes, `shift-tab` to the place before, and
/// a page at a time.
pub fn left_to_help() -> Legend {
    Legend::new()
        .key("j k", NEXT_PLACE)
        .key("shift-tab", PLACE_BEFORE)
        .key("pgup pgdn", BY_THE_PAGE)
}
