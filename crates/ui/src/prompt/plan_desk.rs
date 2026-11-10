//! A plan being read at its desk, on the terminal's other screen. Pure: a stroke goes in,
//! and what it asks of the prompt comes out.
//!
//! Numbers choose, and `enter` confirms: in the card one key answers because a person is
//! looking at the question; here they are reading with their hands on the keyboard.

use surco::components::{ChoiceRow, DeskFoot, FootRow, Legend, PaneLaid};
use surco::doc::Block;
use surco::look::Look;
use yunta_render::page::desk::{Reading, Spot, View};
use yunta_render::prompt::desk::{self, DeskKeys};
use yunta_render::prompt::Escape;

use super::{Answerable, Press, Stroke};

/// The most options a number can name: one key each, since a second digit would have
/// nothing to say the first has ended. The rest are on the card, and the keys say so.
const NUMBERED: usize = 9;

/// What a decision is asked with, on the desk.
pub struct Asked {
    pub question: String,
    /// The node it is on.
    pub node: Option<String>,
    /// The run's handle.
    pub run: Option<String>,
    /// The workflow and the mode the run follows.
    pub called: Option<String>,
}

/// Where Escape goes from the desk.
#[derive(Clone, Copy)]
pub(crate) enum Leaving {
    /// Back to the card it opened from.
    ToTheCard,
    /// Out of the command that opened it, with what the console's Escape does.
    TheCommand(Escape),
}

/// What ended a stay at the desk.
#[derive(Debug, PartialEq, Eq)]
pub enum Left {
    /// The person is done reading.
    Back,
    /// The person chose this option, by its place on the card.
    Picked(usize),
    /// The person stops the run. It is told once the other screen is gone, so what the run
    /// says about being stopped stays on the screen that stays.
    Interrupted,
}

/// A plan being read.
pub struct PlanDesk<'a> {
    reading: Reading<'a>,
    asked: Asked,
    offered: Vec<ChoiceRow>,
    leaving: Leaving,
    spot: Spot,
    /// The option a number asked for, waiting to be confirmed.
    confirming: Option<usize>,
    /// What a surface last showed of the pane.
    laid: PaneLaid,
}

impl<'a> PlanDesk<'a> {
    /// The desk of `reading` at its first stop, the plan's own, with nothing waiting to be
    /// confirmed.
    pub(crate) fn new(
        reading: Reading<'a>,
        asked: Asked,
        offered: Vec<ChoiceRow>,
        leaving: Leaving,
    ) -> Self {
        Self {
            reading,
            asked,
            offered,
            leaving,
            spot: Spot {
                at: 0,
                view: View::Stop,
                top: 0,
                choice: 0,
            },
            confirming: None,
            laid: PaneLaid::default(),
        }
    }

    /// The title the desk is headed with: the plan, then the run's handle and the workflow
    /// it follows, each when there is one.
    fn title(&self) -> Vec<String> {
        desk::title(self.asked.run.clone(), self.asked.called.clone())
    }

    /// What stands on the desk's far edge: what needs the person, and the node it is on.
    fn edge(&self) -> String {
        desk::edge(self.asked.node.as_deref())
    }

    /// The foot the desk is drawn with: the question, the options or the one being
    /// confirmed, and the keys that do something here.
    fn foot(&self) -> DeskFoot {
        DeskFoot {
            question: self.asked.question.clone(),
            row: self.foot_row(),
            legend: self.foot_legend(),
        }
    }

    /// The row of the foot: every option a number can name, or the one a number asked for.
    fn foot_row(&self) -> FootRow {
        match self.confirming.and_then(|option| self.offered.get(option)) {
            Some(option) => FootRow::Confirming(option.clone()),
            None => FootRow::Options {
                named: self
                    .offered
                    .iter()
                    .take(NUMBERED)
                    .map(|row| row.name.clone())
                    .collect(),
                more: self.offered.len().saturating_sub(NUMBERED),
            },
        }
    }

    /// The keys that do something from where the desk stands, each with what it does.
    fn foot_legend(&self) -> Legend {
        let at = self.spot.at;
        desk::legend(&DeskKeys {
            confirming: self.confirming.is_some(),
            view: self.spot.view,
            has_choices: self.reading.choices() > 0,
            has_proof: self.reading.has_proof(at),
            says_decided: self.reading.says_what_it_decided(),
            leaving: match self.leaving {
                Leaving::ToTheCard => None,
                Leaving::TheCommand(escape) => Some(escape),
            },
        })
    }

    /// The option a digit names, by its place on the card: `1` is the first. A digit past
    /// the last option names none.
    fn option_named(&self, digit: char) -> Option<usize> {
        let place = digit.to_digit(10)? as usize;
        place
            .checked_sub(1)
            .filter(|option| *option < self.offered.len().min(NUMBERED))
    }

    /// Makes the option a digit names the one waiting to be confirmed. A digit that names
    /// none changes nothing.
    fn name_by_digit(&mut self, digit: char) {
        if let Some(option) = self.option_named(digit) {
            self.confirming = Some(option);
        }
    }

    /// What a stroke asks while an option waits to be confirmed: Enter takes it, Escape or
    /// `q` takes the number back, another number changes it, and Ctrl-C stops the run.
    fn press_confirming(&mut self, option: usize, stroke: Stroke) -> Press<Left> {
        match stroke {
            Stroke::Enter => {
                self.confirming = None;
                Press::Answer(Left::Picked(option))
            }
            Stroke::Decline | Stroke::Insert('q') => {
                self.confirming = None;
                Press::Stay
            }
            Stroke::Insert(digit @ '1'..='9') => {
                self.name_by_digit(digit);
                Press::Stay
            }
            Stroke::Interrupt => Press::Answer(Left::Interrupted),
            _ => Press::Stay,
        }
    }

    /// What a stroke asks while the desk is being read: `q` and Escape leave it, Ctrl-C
    /// stops the run, a number waits for its confirmation, and the rest moves through it.
    fn press_reading(&mut self, stroke: Stroke) -> Press<Left> {
        match stroke {
            Stroke::Decline | Stroke::Insert('q') => Press::Answer(Left::Back),
            Stroke::Interrupt => Press::Answer(Left::Interrupted),
            Stroke::Insert(digit @ '1'..='9') => {
                self.name_by_digit(digit);
                Press::Stay
            }
            other => {
                self.read_key(other);
                Press::Stay
            }
        }
    }

    /// Dispatches a key that moves through the desk: the walk, the two turns and the scroll.
    fn read_key(&mut self, stroke: Stroke) {
        match stroke {
            Stroke::Insert('j') | Stroke::Tab => self.walk(true),
            Stroke::Insert('k') | Stroke::BackTab => self.walk(false),
            Stroke::Insert('p') => self.turn_proof(),
            Stroke::Insert('d') => self.turn_decisions(),
            other => self.scroll(other),
        }
    }

    /// Moves to the next place, or the one before, or under the choices to the next decision
    /// or the one before. Around at either end, with the pane back at its top.
    fn walk(&mut self, forward: bool) {
        if matches!(self.spot.view, View::Choices) {
            self.spot.choice = around(self.spot.choice, self.reading.choices(), forward);
        } else {
            self.spot.at = around(self.spot.at, self.reading.stops(), forward);
            if matches!(self.spot.view, View::Proof) && !self.reading.has_proof(self.spot.at) {
                self.spot.view = View::Stop;
            }
        }
        self.spot.top = 0;
    }

    /// `p` turns the place over to its proof, or back to what it says. A place with no proof
    /// has nothing to turn to.
    fn turn_proof(&mut self) {
        if !self.reading.has_proof(self.spot.at) {
            return;
        }
        self.spot.view = match self.spot.view {
            View::Proof => View::Stop,
            _ => View::Proof,
        };
        self.spot.top = 0;
    }

    /// `d` opens what was decided at this place, or closes it. A place that decided nothing
    /// has nothing to open.
    fn turn_decisions(&mut self) {
        if !self.reading.says_what_it_decided() {
            return;
        }
        self.spot.view = match self.spot.view {
            View::Choices => View::Stop,
            _ => View::Choices,
        };
        self.spot.top = 0;
    }

    /// Scrolls the pane by the keys that move through what is read, never past what the last
    /// drawing showed to be the end. A page moves by what was in sight, which leaves out the
    /// row that counts what is below, so a person who pages through it reads every line. A
    /// key that scrolls nothing is left alone.
    fn scroll(&mut self, stroke: Stroke) {
        let PaneLaid { top, page, most } = self.laid;
        let in_sight = page.saturating_sub(1).max(1);
        self.spot.top = match stroke {
            Stroke::Down => (top + 1).min(most),
            Stroke::Up => top.saturating_sub(1),
            Stroke::PageDown | Stroke::Insert(' ') => (top + in_sight).min(most),
            Stroke::PageUp => top.saturating_sub(in_sight),
            Stroke::Home => 0,
            Stroke::End => most,
            _ => return,
        };
    }
}

/// The place after `at` in a list of `count` places, or the one before it, around at the
/// ends. A list with nothing in it counts as one place, so `at` stays where it is.
fn around(at: usize, count: usize, forward: bool) -> usize {
    let count = count.max(1);
    if forward {
        (at + 1) % count
    } else {
        (at + count - 1) % count
    }
}

impl Answerable for PlanDesk<'_> {
    type Answer = Left;

    /// The desk as it stands: the plan at the place it stands on, and its foot.
    fn block(&self, look: &Look) -> Block {
        Block::Desk(self.reading.desk(
            &self.spot,
            &self.title(),
            Some(self.edge()),
            self.foot(),
            look,
        ))
    }

    /// What a surface last showed of the pane, which the scroll keys move from.
    fn laid(&mut self, pane: PaneLaid) {
        self.laid = pane;
    }

    /// What a stroke asks of the desk: a number waits for its confirmation, and every other
    /// stroke moves the desk or leaves it.
    fn press(&mut self, stroke: Stroke) -> Press<Left> {
        match self.confirming {
            Some(option) => self.press_confirming(option, stroke),
            None => self.press_reading(stroke),
        }
    }

    /// Nothing typed here is words, so a paste answers nothing.
    fn typing(&self) -> bool {
        false
    }
}

#[cfg(test)]
pub(crate) mod tests;
