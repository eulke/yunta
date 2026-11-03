//! The decision a person makes when the run stops and asks.
//!
//! The decision is a card held where it opened: its question, what a reader
//! needs to answer it, and each option with what choosing it does and costs,
//! one pointed at. Arrows move the pointer, Enter chooses, `n` adds a note and
//! Escape parks the run. What the card stands under is drawn above it, when
//! there is anything to draw: the documents the decision is about, and what
//! weighs on it. The card itself lives in [`super::card`]; this module gathers
//! what it is drawn from and puts it to the person.

use std::collections::BTreeMap;

use surco::glyphs::Glyphs;
use yunta_core::events::{ChangesCountedPayload, GateWaitingPayload, HumanChoice};
use yunta_core::{Env, NodeId, OptionId, RunId};
use yunta_engine::ShownDocument;
use yunta_render::changes;
use yunta_render::gate::{self, Route};

use super::card::{Answer, DecisionCard};
use super::field::FOLDED;
use super::held::hold;
use super::{attributed, Answered, Console};
use crate::commands::advice;

/// What a decision is asked with: the escalation, the documents it shows in the run
/// they belong to, what the run changed, what follows each answer, and the node it
/// is on.
pub(crate) struct Ask<'a> {
    pub(crate) escalation: &'a GateWaitingPayload,
    pub(crate) shown: Option<(&'a RunId, &'a [ShownDocument])>,
    pub(crate) changed: Option<&'a ChangesCountedPayload>,
    pub(crate) routes: BTreeMap<OptionId, Route>,
    pub(crate) node: Option<&'a NodeId>,
}

/// Puts the decision to the person as a card held where it opened, and returns what
/// they chose. `env` is the ambient identity the choice is attributed to.
pub(crate) fn decide(console: &Console, env: &Env, ask: Ask<'_>) -> Answered<HumanChoice> {
    present(console, &ask)?;
    let glyphs = console.look().glyphs;
    let (edge, about) = (edge(&ask, glyphs), about(&ask, glyphs));
    let mut card =
        DecisionCard::new(ask.escalation, console.escape(), ask.routes).standing_on(edge, about);
    let answer = hold(console, &mut card)?;
    console.say(&chose(&answer))?;
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

/// Draws what the card stands under, above it: each document the decision is about,
/// in the form a terminal shows it, and what weighs on it. Nothing is drawn when
/// there is neither, since the card opens on its question.
fn present(console: &Console, ask: &Ask<'_>) -> std::io::Result<()> {
    let Some(page) = gate::above(ask.shown, ask.escalation, &advice::spell) else {
        return Ok(());
    };
    let look = console.look();
    for line in crate::render::draw(page, &look).lines() {
        console.say(line)?;
    }
    console.say("")
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
/// when the change was counted.
fn about(ask: &Ask<'_>, glyphs: Glyphs) -> Vec<String> {
    let documents = ask.shown.map_or(&[][..], |(_, documents)| documents);
    let mut about = gate::record(ask.escalation, documents);
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
