//! The decision a person makes when the run stops and asks.
//!
//! The decision is a card held where it opened: its question, what a reader
//! needs to answer it, and each option with what choosing it does and costs,
//! one pointed at. Arrows move the pointer, Enter chooses, `n` adds a note and
//! Escape parks the run. What the card stands under is drawn above it, when
//! there is anything to draw: the documents the desk does not show, and what
//! weighs on it. `r` reads the plan the decision is on, at the desk on the
//! terminal's other screen; a person who came to read a plan opens on that
//! plan first. The card itself lives in [`super::card`] and the desk in
//! [`super::desk`]; this module gathers what they are drawn from and puts them
//! to the person.

use std::collections::BTreeMap;

use surco::doc::Doc;
use surco::glyphs::Glyphs;
use yunta_core::events::{ChangesCountedPayload, GateWaitingPayload, HumanChoice};
use yunta_core::shown::ShownDocument;
use yunta_core::{Env, NodeId, OptionId, RunId};
use yunta_render::escalation::{self, Route};
use yunta_render::gate;
use yunta_render::{changes, plan};

use super::card::{Answer, DecisionCard, Reply};
use super::desk::{Asked, Leaving, Left, PlanDesk};
use super::field::FOLDED;
use super::held::hold;
use super::{attributed, Answered, Console, NoAnswer};

/// The fewest columns a desk is drawn on: under them a task's title and where it is no longer
/// share a row, and code wraps at every word.
const DESK_COLUMNS: usize = 40;

/// The fewest rows a desk is drawn on: its head, its foot and the rows left for a person to
/// read something between them.
const DESK_ROWS: usize = 12;

/// What a decision is asked with: the escalation, the documents it shows in the run
/// they belong to, what the run changed, what follows each answer, the node it is
/// on, the workflow and mode the run follows, and what it opens on.
pub(crate) struct Ask<'a> {
    pub(crate) escalation: &'a GateWaitingPayload,
    pub(crate) shown: Option<(&'a RunId, &'a [ShownDocument])>,
    pub(crate) changed: Option<&'a ChangesCountedPayload>,
    pub(crate) routes: BTreeMap<OptionId, Route>,
    pub(crate) node: Option<&'a NodeId>,
    /// The workflow and the mode the run follows.
    pub(crate) called: Option<String>,
    pub(crate) opens: Opens,
}

/// What a decision opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opens {
    /// Its card, with the plan a key away.
    TheCard,
    /// The plan it is on, for a person who came to read it.
    ThePlan,
}

/// Puts the decision to the person as a card held where it opened, and returns what
/// they chose. `env` is the ambient identity the choice is attributed to.
pub(crate) fn decide(console: &Console, env: &Env, mut ask: Ask<'_>) -> Answered<HumanChoice> {
    present(console, &ask)?;
    let glyphs = console.look().glyphs;
    let (edge, about) = (edge(&ask, glyphs), about(&ask, glyphs));
    let routes = std::mem::take(&mut ask.routes);
    let mut card =
        DecisionCard::new(ask.escalation, console.escape(), routes).standing_on(edge, about);
    let mut desk = desk_of(console, &ask, &card);
    if desk.is_some() {
        card = card.reading();
    }
    let answer = answered(console, &ask, &mut card, desk.as_mut())?;
    console.say_unless_chronicled(&chose(&answer))?;
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

/// The desk of the plan `ask` is on, when it is on one.
fn desk_of<'a>(console: &Console, ask: &Ask<'a>, card: &DecisionCard) -> Option<PlanDesk<'a>> {
    let (run, documents) = ask.shown?;
    let reading = plan::reading(documents, run)?;
    let asked = Asked {
        question: question(ask),
        node: ask.node.map(ToString::to_string),
        run: Some(run.handle().to_string()),
        called: ask.called.clone(),
    };
    let leaving = match ask.opens {
        Opens::TheCard => Leaving::ToTheCard,
        Opens::ThePlan => Leaving::TheCommand(console.escape()),
    };
    Some(PlanDesk::new(reading, asked, card.rows(), leaving))
}

/// The card and the desk, until the decision is answered.
fn answered(
    console: &Console,
    ask: &Ask<'_>,
    card: &mut DecisionCard,
    mut desk: Option<&mut PlanDesk<'_>>,
) -> Answered<Answer> {
    if ask.opens == Opens::ThePlan {
        if let Some(answer) = read(console, ask, card, desk.as_deref_mut())? {
            return Ok(answer);
        }
    }
    loop {
        match hold(console, card)? {
            Reply::Chose(answer) => return Ok(answer),
            Reply::Read => {
                if let Some(answer) = read(console, ask, card, desk.as_deref_mut())? {
                    return Ok(answer);
                }
            }
        }
    }
}

/// A stay at the desk: what it answered, or `None` where the card has more to say. A
/// terminal with no room for a desk gets the plan in its history, over the card. Ctrl-C
/// reaches the run only once the other screen is gone, so what it says stands on the
/// screen that stays.
fn read(
    console: &Console,
    ask: &Ask<'_>,
    card: &mut DecisionCard,
    desk: Option<&mut PlanDesk<'_>>,
) -> Answered<Option<Answer>> {
    let Some(desk) = desk else {
        return Ok(None);
    };
    let (columns, rows) = console.size();
    if columns < DESK_COLUMNS || rows < DESK_ROWS {
        print_documents(console, ask)?;
        return Ok(None);
    }
    console.alternate_on()?;
    let left = hold(console, desk);
    console.alternate_off()?;
    match left? {
        Left::Back if ask.opens == Opens::ThePlan => Err(NoAnswer::Declined),
        Left::Back => Ok(None),
        Left::Picked(index) => Ok(card.pick(index)),
        Left::Interrupted => {
            console.interrupt();
            Err(NoAnswer::Interrupted)
        }
    }
}

/// Draws what the card stands under, above it: each document the decision is about that
/// the desk does not show, in the form a terminal shows it, and what weighs on it. A plan
/// among the documents is read at the desk and draws nothing here. Nothing is drawn when
/// there is neither, since the card opens on its question.
fn present(console: &Console, ask: &Ask<'_>) -> std::io::Result<()> {
    let Some(page) = gate::above(ask.shown, ask.escalation) else {
        return Ok(());
    };
    draw_page(console, page)
}

/// Draws everything the decision has to say, what weighs on it last, for a terminal with no
/// room for the desk.
fn print_documents(console: &Console, ask: &Ask<'_>) -> std::io::Result<()> {
    let Some(page) = gate::whole_page(ask.shown, ask.escalation) else {
        return Ok(());
    };
    draw_page(console, page)
}

/// Draws `page` the way a prompt draws what it stands under: each line said, then a
/// blank line that sets the card apart.
fn draw_page(console: &Console, page: Doc) -> std::io::Result<()> {
    let look = console.look();
    for line in crate::output::draw(page, &look).lines() {
        console.say(line)?;
    }
    console.say("")
}

/// The sentence that says what the decision is, as the card and the desk both put it.
fn question(ask: &Ask<'_>) -> String {
    yunta_render::claim::sentence(ask.escalation.summary(), ask.escalation.failure())
}

/// What stands on the card's far edge: the node the decision is on and the run's
/// handle, apart by a separator, whichever of them there are.
fn edge(ask: &Ask<'_>, glyphs: Glyphs) -> Option<String> {
    let parts: Vec<String> = ask
        .node
        .map(|node| node.to_string())
        .into_iter()
        .chain(ask.shown.map(|(run, _)| run.handle().to_string()))
        .collect();
    let separator = format!(" {} ", glyphs.sep());
    (!parts.is_empty()).then(|| parts.join(separator.as_str()))
}

/// What a reader needs to answer, under the question: the record the engine attached
/// to it, audited against the documents it stands under, then what the run changed
/// when the change was counted. A plan the decision is on adds what the plan says about
/// the question.
fn about(ask: &Ask<'_>, glyphs: Glyphs) -> Vec<String> {
    let documents = ask.shown.map_or(&[][..], |(_, documents)| documents);
    let question = question(ask);
    let mut about: Vec<String> = yunta_render::shown::plan_among(documents)
        .map(|(_, review)| plan::about(review, &question, glyphs))
        .unwrap_or_default();
    about.extend(escalation::record(ask.escalation, documents));
    about.extend(ask.changed.and_then(|change| changes::of(change, glyphs)));
    about
}

/// The line a choice is kept as in the history: the option, and the words it was
/// said with, as the person wrote them.
fn chose(answer: &Answer) -> String {
    match &answer.words {
        Some(words) => format!("chose `{}`: {words}", answer.option),
        None => format!("chose `{}`", answer.option),
    }
}
