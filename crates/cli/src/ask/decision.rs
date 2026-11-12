//! The decision a person makes when the run stops and asks, held at a terminal.
//!
//! The decision is `yunta_ui`'s [`Deciding`]: its card, and the plan it is on a key
//! away, with the turns between them. This module holds the one in front where a
//! terminal can: the card where it opened, the desk on the terminal's other screen, and
//! the plan printed in the history where the terminal has no room for a desk. What the
//! card stands under is drawn above it, when there is anything to draw: the documents
//! the desk does not show, and what weighs on it. What the card says is render's
//! `prompt::card`.

use yunta_core::events::HumanChoice;
use yunta_core::shown::ShownDocument;
use yunta_core::{Env, RunId};
use yunta_render::page::{Gate, Page};
use yunta_render::prompt::{card, FOLDED};
use yunta_ui::prompt::{Deciding, DecisionPut, InFront, Opens, PickedOption, Press};

use super::held::hold;
use super::{attributed, Answered, Console, NoAnswer};

/// The fewest columns a desk is drawn on: under them a task's title and where it is no longer
/// share a row, and code wraps at every word.
const DESK_COLUMNS: usize = 40;

/// The fewest rows a desk is drawn on: its head, its foot and the rows left for a person to
/// read something between them.
const DESK_ROWS: usize = 12;

/// Puts the decision `put` to the person as a card held where it opens, the plan's desk a
/// key away, and returns what they chose. `env` is the ambient identity the choice is
/// attributed to.
pub(crate) fn decide(console: &Console, env: &Env, put: &DecisionPut) -> Answered<HumanChoice> {
    present(console, put)?;
    let mut deciding = Deciding::put(put, Opens::TheCard, console.escape());
    let answer = answered(console, put, &mut deciding)?;
    console.say_unless_chronicled(&card::chose(&answer.option, answer.words.as_deref()))?;
    if answer.folded {
        console.say(FOLDED)?;
    }
    let by = attributed(console, env)?;
    Ok(HumanChoice {
        option: answer.option,
        by,
        free_text: answer.words,
    })
}

/// The one in front, held until the decision is answered: the card where it opened,
/// and the desk on the terminal's other screen. A terminal with no room for a desk gets
/// the plan in its history, over the card. Ctrl-C at the desk reaches the run only once
/// the other screen is gone, so what it says stands on the screen that stays.
fn answered(
    console: &Console,
    put: &DecisionPut,
    deciding: &mut Deciding,
) -> Answered<PickedOption> {
    loop {
        match deciding.in_front() {
            InFront::TheCard(card) => {
                let reply = hold(console, card)?;
                if let Some(answer) = deciding.replied(reply) {
                    return Ok(answer);
                }
            }
            InFront::ThePlan(desk) => {
                let (columns, rows) = console.size();
                if columns < DESK_COLUMNS || rows < DESK_ROWS {
                    print_documents(console, put)?;
                    deciding.plan_unshown();
                    continue;
                }
                console.alternate_on()?;
                let left = hold(console, desk);
                console.alternate_off()?;
                match deciding.left(left?) {
                    Press::Answer(answer) => return Ok(answer),
                    Press::Decline => return Err(NoAnswer::Declined),
                    Press::Interrupt => {
                        console.interrupt();
                        return Err(NoAnswer::Interrupted);
                    }
                    Press::Stay | Press::Moved => {}
                }
            }
        }
    }
}

/// Draws what the card stands under, above it: each document the decision is about that
/// the desk does not show, in the form a terminal shows it, and what weighs on it. A plan
/// among the documents is read at the desk and draws nothing here. Nothing is drawn when
/// there is neither, since the card opens on its question.
fn present(console: &Console, put: &DecisionPut) -> std::io::Result<()> {
    let page = Gate::AboveTheCard {
        shown: shown(put),
        escalation: &put.escalation,
    };
    draw_page(console, &page)
}

/// Draws everything the decision has to say, what weighs on it last, for a terminal with no
/// room for the desk.
fn print_documents(console: &Console, put: &DecisionPut) -> std::io::Result<()> {
    let page = Gate::InPlaceOfTheDesk {
        shown: shown(put),
        escalation: &put.escalation,
    };
    draw_page(console, &page)
}

/// The documents `put` shows, in the run they belong to, as a gate's page reads them.
fn shown(put: &DecisionPut) -> Option<(&RunId, &[ShownDocument])> {
    put.shown
        .as_ref()
        .map(|(run, documents)| (run, documents.as_slice()))
}

/// Draws `page` the way a prompt draws what it stands under: each line said, then a
/// blank line that sets the card apart. A page with nothing to say draws nothing.
fn draw_page(console: &Console, page: &impl Page) -> std::io::Result<()> {
    let look = console.look();
    let page = page.document(&look);
    if page.blocks().is_empty() {
        return Ok(());
    }
    for line in crate::output::draw(page, &look).lines() {
        console.say(line)?;
    }
    console.say("")
}
