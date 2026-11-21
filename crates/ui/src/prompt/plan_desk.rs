//! A plan being read at its desk, on the terminal's other screen. Pure: a stroke goes in,
//! and what it asks of the prompt comes out.
//!
//! Numbers choose, and `enter` confirms: in the card one key answers because a person is
//! looking at the question; here they are reading with their hands on the keyboard. An
//! option that asks for words, confirmed, opens its line at the desk's foot in place of the
//! options, the desk standing: every key types there, `enter` answers with the words, an
//! empty line refused as the card refuses it, and `esc` goes back to the options.
//!
//! The terminal's other screen opens it on the place read first, the change where the plan
//! says what it changes as a whole; a run's screen opens it on the plan's first task
//! ([`PlanDesk::on_its_first_task`]) and holds it beside keys of its own, handing it only the
//! keys it reads by, and `esc` where it goes back to the card it was read from
//! ([`PlanDesk::takes`]), and saying them in the screen's legend
//! ([`PlanDesk::legend_on_a_screen`]).

use surco::components::{DeskFoot, FootRow, Laid, Legend};
use surco::doc::Block;
use surco::look::Look;
use yunta_render::page::desk::{Reading, Spot, View};
use yunta_render::prompt::{card, confirming_legend, desk, Escape, PromptAct, ESC};

use super::decision_card::Offered;
use super::{Answerable, Edit, PickedOption, Press, Stroke, TypedLine};
use crate::scroll::Scroll;

/// The most options a number can name: one key each, since a second digit would have
/// nothing to say the first has ended. The rest are on the card, and the keys say so.
const NUMBERED: usize = 9;

/// What a decision is asked with, on the desk.
#[derive(Debug, Clone)]
pub struct Asked {
    pub question: String,
    /// The node it is on.
    pub node: Option<String>,
    /// The run's handle.
    pub run: Option<String>,
    /// The workflow the run follows, by its name.
    pub workflow: Option<String>,
}

/// Where Escape goes from the desk.
#[derive(Debug, Clone, Copy)]
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
    /// The person chose an option, with the words it asks for where it asks for them.
    Chose(PickedOption),
    /// The person stops the run. It is told once the other screen is gone, so what the run
    /// says about being stopped stays on the screen that stays.
    Interrupted,
}

/// A plan being read.
#[derive(Debug, Clone)]
pub struct PlanDesk {
    reading: Reading,
    asked: Asked,
    offered: Vec<Offered>,
    leaving: Leaving,
    spot: Spot,
    answering: Answering,
    /// Where the pane is scrolled to, and what a surface last showed of it.
    scroll: Scroll,
}

/// How the question at the desk's foot stands, each option by its place among those
/// offered. Not [`FootRow`], what a surface is handed to draw of it.
#[derive(Debug, Clone)]
enum Answering {
    /// Open to a number.
    Open,
    /// The option a number named, waiting to be confirmed.
    Confirming(usize),
    /// Words being typed for an option that asks for them, and whether the line was
    /// refused for holding none.
    Saying {
        option: usize,
        line: TypedLine,
        refused: bool,
    },
}

impl PlanDesk {
    /// The desk of `reading` at its first stop, the plan's own, offering `offered`, with
    /// nothing waiting to be confirmed.
    pub(crate) fn new(
        reading: Reading,
        asked: Asked,
        offered: Vec<Offered>,
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
            },
            answering: Answering::Open,
            scroll: Scroll::default(),
        }
    }

    /// It standing on the plan's first task, as a run's screen opens it, where the plan has
    /// one; where it reads first what the plan says of itself or what else the decision shows,
    /// that is a key back.
    pub(crate) fn on_its_first_task(mut self) -> Self {
        if let Some(at) = self.reading.first_task() {
            self.spot.at = at;
        }
        self
    }

    /// Whether `stroke` means anything to the desk, held beside keys of their own on a screen
    /// that hands it only those: a number that names one of its options, the keys that walk
    /// from place to place, `p` where the place has a proof, `d` where the plan says what it
    /// decided, the keys that move through what is read, and `esc` where it goes back to the
    /// card the plan was read from; while a number waits to be confirmed, `enter` and `esc`
    /// too; and while words are typed at its foot, every key. Every other key is the
    /// screen's, `esc` among them while nothing waits to be confirmed on a desk opened on the
    /// plan, and Ctrl-C while nothing is typed.
    pub(crate) fn takes(&self, stroke: Stroke) -> bool {
        let reads_by = match stroke {
            Stroke::Insert(digit @ '1'..='9') => self.option_named(digit).is_some(),
            Stroke::Insert('j' | 'k') | Stroke::Tab | Stroke::BackTab => true,
            Stroke::Insert('p') => self.reading.has_proof(self.spot.at),
            Stroke::Insert('d') => self.reading.says_what_it_decided(),
            other => Scroll::moves_by(other),
        };
        let confirms = matches!(stroke, Stroke::Enter | Stroke::Decline);
        let to_the_card = matches!(
            (stroke, self.leaving),
            (Stroke::Decline, Leaving::ToTheCard)
        );
        match self.answering {
            Answering::Open => reads_by || to_the_card,
            Answering::Confirming(_) => reads_by || confirms,
            Answering::Saying { .. } => true,
        }
    }

    /// The keys a screen's legend says for it, where the screen holds it beside keys of its
    /// own, in render's words: as its own foot says them, Escape going back to the card the
    /// plan was read from, or else leaving the decision waiting.
    pub(crate) fn legend_on_a_screen(&self) -> Legend {
        let escape = match self.leaving {
            Leaving::ToTheCard => PromptAct::GoBack,
            Leaving::TheCommand(_) => PromptAct::LeaveItWaiting,
        };
        self.legend(escape.said())
    }

    /// Every key it takes on a screen, as help over the screen says them: what the screen's
    /// legend names for it, then what that legend leaves to help: while it is read, `j k`, the
    /// place before and a page at a time; while a number waits to be confirmed, those and
    /// every key it still reads by, as it says them while it is read; nothing more while
    /// words are typed at its foot, where every key types.
    pub(crate) fn keys_for_help(&self) -> Legend {
        let beside = match self.answering {
            Answering::Saying { .. } => Legend::new(),
            Answering::Open => desk::left_to_help(),
            Answering::Confirming(_) => {
                let read_by = desk::legend(
                    self.offered.len(),
                    self.reading.has_proof(self.spot.at),
                    self.reading.says_what_it_decided(),
                    PromptAct::LeaveItWaiting.said(),
                );
                let read_by = read_by.0.into_iter().filter(|key| key.keys.named() != ESC);
                Legend(read_by.chain(desk::left_to_help().0).collect())
            }
        };
        let Legend(keys) = self.legend_on_a_screen();
        Legend(keys.into_iter().chain(beside.0).collect())
    }

    /// The keys that do something from where the desk stands, each with what it does, on a
    /// screen and at its own foot alike: sending the words and going back while they are
    /// typed at its foot; confirming and going back while a number waits to be confirmed;
    /// else a number for each option, the next place, what is read, the proof where the
    /// place has one, what is decided where the plan says it, and `escape`, what Escape does
    /// where it stands.
    fn legend(&self, escape: &str) -> Legend {
        match self.answering {
            Answering::Saying { .. } => card::saying_legend(true),
            Answering::Confirming(_) => confirming_legend(),
            Answering::Open => desk::legend(
                self.offered.len(),
                self.reading.has_proof(self.spot.at),
                self.reading.says_what_it_decided(),
                escape,
            ),
        }
    }

    /// Where its pane is scrolled to, and what a surface last showed of it.
    pub(crate) fn scroll(&self) -> &Scroll {
        &self.scroll
    }

    /// Where its pane is scrolled to, for what a surface laid out of it.
    pub(crate) fn scroll_mut(&mut self) -> &mut Scroll {
        &mut self.scroll
    }

    /// The title the desk is headed with: the plan, then the run's handle and the workflow
    /// it follows, each when there is one.
    fn title(&self) -> Vec<String> {
        desk::title(self.asked.run.clone(), self.asked.workflow.clone())
    }

    /// What stands on the desk's far edge: what needs the person, and the node it is on.
    fn edge(&self) -> String {
        desk::edge(self.asked.node.as_deref())
    }

    /// The foot the desk is drawn with: the question, the options, the one being confirmed
    /// or the one words are typed for, and the keys that do something here.
    fn foot(&self) -> DeskFoot {
        DeskFoot {
            question: self.asked.question.clone(),
            row: self.foot_row(),
            legend: self.foot_legend(),
        }
    }

    /// The row of the foot: the one a number asked for, or the one words are typed for with
    /// the line they are typed on; else every option a number can name.
    fn foot_row(&self) -> FootRow {
        match &self.answering {
            Answering::Confirming(option) => {
                if let Some(offered) = self.offered.get(*option) {
                    return FootRow::Confirming(offered.row.clone());
                }
            }
            Answering::Saying {
                option,
                line,
                refused,
            } => {
                if let Some(
                    offered @ Offered {
                        asks: Some(asks), ..
                    },
                ) = self.offered.get(*option)
                {
                    let row = offered.row.clone();
                    return desk::saying(row, asks.clone(), line.split(), *refused);
                }
            }
            Answering::Open => {}
        }
        FootRow::Options {
            named: self
                .offered
                .iter()
                .take(NUMBERED)
                .map(|offered| offered.row.name.clone())
                .collect(),
            more: self.offered.len().saturating_sub(NUMBERED),
        }
    }

    /// The keys its foot says, Escape going back to the card it opened from, or doing what
    /// it does in the command that opened it.
    fn foot_legend(&self) -> Legend {
        let escape = match self.leaving {
            Leaving::ToTheCard => yunta_render::screen::BACK,
            Leaving::TheCommand(escape) => escape.does(),
        };
        self.legend(escape)
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
            self.answering = Answering::Confirming(option);
        }
    }

    /// What a stroke asks while an option waits to be confirmed: Enter takes it, or opens
    /// its line where it asks for words; Escape or `q` takes the number back, another number
    /// changes it, and Ctrl-C stops the run.
    fn press_confirming(&mut self, option: usize, stroke: Stroke) -> Press<Left> {
        match stroke {
            Stroke::Enter => {
                let Some(offered) = self.offered.get(option) else {
                    self.answering = Answering::Open;
                    return Press::Stay;
                };
                if offered.asks.is_some() {
                    self.answering = Answering::Saying {
                        option,
                        line: TypedLine::default(),
                        refused: false,
                    };
                    return Press::Stay;
                }
                let chosen = offered.chosen();
                self.answering = Answering::Open;
                Press::Answer(Left::Chose(chosen))
            }
            Stroke::Decline | Stroke::Insert('q') => {
                self.answering = Answering::Open;
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

    /// What a stroke asks while words are typed at the foot: it edits them; Enter answers
    /// with them, or says under the line that the option needs some where it holds none;
    /// Escape goes back to the options with nothing kept, and Ctrl-C stops the run.
    fn press_saying(&mut self, stroke: Stroke) -> Press<Left> {
        let Answering::Saying {
            option,
            line,
            refused,
        } = &mut self.answering
        else {
            return Press::Stay;
        };
        match line.apply(stroke) {
            Edit::Editing => {
                *refused = false;
                Press::Stay
            }
            Edit::Answered(typed) => {
                let said = self
                    .offered
                    .get(*option)
                    .and_then(|offered| offered.said_with(typed));
                match said {
                    Some(picked) => Press::Answer(Left::Chose(picked)),
                    None => {
                        *refused = true;
                        Press::Stay
                    }
                }
            }
            Edit::Declined => {
                self.answering = Answering::Open;
                Press::Stay
            }
            Edit::Interrupted => Press::Answer(Left::Interrupted),
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
            other => self.scroll.pressed(other),
        }
    }

    /// Moves to the next place, or the one before, around at either end, with the pane back
    /// at its top: what the place says, or its proof where it was the proof being read and the
    /// place has one. What was decided is no place's, so walking leaves it for the place.
    fn walk(&mut self, forward: bool) {
        self.spot.at = around(self.spot.at, self.reading.stops(), forward);
        self.spot.view = match self.spot.view {
            View::Proof if self.reading.has_proof(self.spot.at) => View::Proof,
            View::Proof | View::Stop | View::Choices => View::Stop,
        };
        self.scroll.rewound();
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
        self.scroll.rewound();
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
        self.scroll.rewound();
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

impl Answerable for PlanDesk {
    type Answer = Left;

    /// The desk as it stands: the plan at the place it stands on, and its foot.
    fn block(&self, look: &Look) -> Block {
        Block::from(self.reading.desk(
            &self.spot,
            self.scroll.sight().top,
            &self.title(),
            Some(self.edge()),
            self.foot(),
            look,
        ))
    }

    /// What a surface last showed of the pane, which the scroll keys move from.
    fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// What a stroke asks of the desk: a number waits for its confirmation, words typed at
    /// its foot take every stroke, and every other stroke moves the desk or leaves it.
    fn press(&mut self, stroke: Stroke) -> Press<Left> {
        match self.answering {
            Answering::Open => self.press_reading(stroke),
            Answering::Confirming(option) => self.press_confirming(option, stroke),
            Answering::Saying { .. } => self.press_saying(stroke),
        }
    }

    /// Whether words are typed at its foot: what is typed or pasted elsewhere on the desk is
    /// no words, so a paste answers nothing there.
    fn typing(&self) -> bool {
        matches!(self.answering, Answering::Saying { .. })
    }
}

#[cfg(test)]
pub(crate) mod tests;
