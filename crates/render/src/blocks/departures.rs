//! What a card on a task's departure from its plan rests on: for each departure a session
//! declared, what the plan decided, what the session says instead, and why, each quoted under
//! its words from the gutter as the session declared it. Short of rows, why gives way first:
//! what the plan decided and what the session did instead are what the person answers.
//!
//! The departures are the escalation's: a departure asked at the engine's terminal carries
//! them in its menu, and a loop whose tasks each stopped on a departure owed a person carries
//! them in its failure, which `Failure::departures_owed` says is one. Where a menu asks
//! anything else, there is none.

use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::line::{Line, Tone};
use yunta_core::events::{DeviationDeclaredPayload, Failure, GateWaitingPayload, Menu, TaskCause};

use super::decision::{BECAUSE, SAYS_INSTEAD, THE_PLAN_DECIDED};

/// The departures from the plan a decision asks about, as its card rests on them. Not
/// [`yunta_core::events::DepartureMenu`], the engine's menu that asks one, nor the record a
/// printed page lists of it.
pub struct Departures<'a> {
    /// Each departure, in the order the sessions declared them.
    declared: Vec<&'a DeviationDeclaredPayload>,
}

impl<'a> Departures<'a> {
    /// The departures `escalation` asks about: those its departure's menu carries, or those
    /// of the tasks its failure stopped each on a departure owed. None for any other
    /// escalation.
    pub fn asked_by(escalation: &'a GateWaitingPayload) -> Departures<'a> {
        let declared = match escalation.menu() {
            Some(Menu::Departure(menu)) => menu.deviations.iter().collect(),
            Some(Menu::Failure(menu)) => owed(&menu.failure),
            Some(
                Menu::Gate(_)
                | Menu::Console(_)
                | Menu::Published(_)
                | Menu::Scope(_)
                | Menu::Budget(_)
                | Menu::LoopCap(_),
            )
            | None => Vec::new(),
        };
        Departures { declared }
    }

    /// Its blocks, in `glyphs`: for each departure, what the plan decided, what the session
    /// says instead and why, each under its words, its own lines hanging from the gutter.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        self.said(true, glyphs)
    }

    /// Its blocks said again in fewer rows, over a card short of them: each departure's why
    /// given way, what the plan decided and what the session says instead standing.
    pub fn abridged(&self, glyphs: Glyphs) -> Vec<Vec<Block>> {
        match self.declared.is_empty() {
            true => Vec::new(),
            false => vec![self.said(false, glyphs)],
        }
    }

    /// For each departure, what the plan decided and what the session says instead, then
    /// why where `why` says, each under its words, in `glyphs`.
    fn said(&self, why: bool, glyphs: Glyphs) -> Vec<Block> {
        self.declared
            .iter()
            .flat_map(|departure| {
                let because = why.then_some((BECAUSE, departure.why.as_str()));
                [
                    (THE_PLAN_DECIDED, departure.planned.as_str()),
                    (SAYS_INSTEAD, departure.instead.as_str()),
                ]
                .into_iter()
                .chain(because)
            })
            .flat_map(|(words, said)| {
                [
                    Block::Heading(words.to_string()),
                    Block::Rows(quoted(said, glyphs)),
                ]
            })
            .collect()
    }
}

/// The departures of the tasks `failure` stopped, where it stopped each on a departure owed a
/// person; none for any other failure.
fn owed(failure: &Failure) -> Vec<&DeviationDeclaredPayload> {
    let (Failure::TasksStopped { stopped }, Some(_)) = (failure, failure.departures_owed()) else {
        return Vec::new();
    };
    stopped
        .iter()
        .flat_map(|stop| match &stop.cause {
            TaskCause::DeviationOwed { deviations } => deviations.as_slice(),
            _ => &[],
        })
        .collect()
}

/// The lines of `said`, each hanging from the gutter in `glyphs`.
fn quoted(said: &str, glyphs: Glyphs) -> Vec<Line> {
    let gutter = format!("{} ", glyphs.gutter());
    said.lines()
        .map(|line| Line::new().push(Tone::Muted, gutter.as_str()).plain(line))
        .collect()
}

#[cfg(test)]
mod tests;
