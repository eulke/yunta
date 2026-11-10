//! A decision a person answers with keys, as a card: the question, what a reader
//! needs to answer, every option with what it does and what it costs, one pointed
//! at, and what follows it. Pure: a stroke goes in, and what it asks of the prompt
//! comes out. No option names a command: the prompt is how one is chosen, and the
//! commands are said where the run is parked.

use std::collections::BTreeMap;

use surco::components::{Card, CardBody, ChoiceRow, Choices, Legend};
use surco::doc::Block;
use surco::look::Look;
use yunta_core::events::GateWaitingPayload;
use yunta_core::OptionId;
use yunta_render::escalation::Route;
use yunta_render::prompt::{card, Escape};

use super::{Answerable, Edit, Press, Stroke, TypedLine};

/// The option a person chose on a decision's card, and the words it was said with. Not
/// core's answer to a question, nor what an attention asks for.
#[derive(Debug, PartialEq, Eq)]
pub struct PickedOption {
    pub option: OptionId,
    pub words: Option<String>,
    /// Whether the words were pasted over more than one line and folded into one.
    pub folded: bool,
}

/// What a person did at the card.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    /// They answered it.
    Chose(PickedOption),
    /// They asked to read the plan it decides on.
    Read,
}

/// One option as the card holds it.
struct Offered {
    id: OptionId,
    row: ChoiceRow,
    /// What it asks for, when it needs words.
    asks: Option<String>,
}

/// The words an option is being said with.
struct Saying {
    field: TypedLine,
    refused: bool,
}

/// A decision being answered.
pub struct DecisionCard {
    question: String,
    edge: Option<String>,
    about: Vec<String>,
    offered: Vec<Offered>,
    routes: BTreeMap<OptionId, Route>,
    at: usize,
    saying: Option<Saying>,
    escape: Escape,
    /// Whether the plan the decision is on can be read from here, a key away.
    reads: bool,
}

impl DecisionCard {
    /// The card for `escalation`: its question, and every option it offers with what
    /// it says and what it costs, the pointer on the first. `routes` says what
    /// follows each option that leads somewhere.
    pub fn new(
        escalation: &GateWaitingPayload,
        escape: Escape,
        routes: BTreeMap<OptionId, Route>,
    ) -> Self {
        let offered = escalation
            .options()
            .iter()
            .zip(yunta_render::offers::decision_options(escalation))
            .map(|(option, words)| {
                let said = words.said_label().map(str::to_string);
                Offered {
                    id: option.id.clone(),
                    row: ChoiceRow {
                        name: words.id,
                        said,
                        cost: words.tradeoff,
                    },
                    asks: words.asks,
                }
            })
            .collect();
        Self {
            question: yunta_render::escalation::claim_words(escalation),
            edge: None,
            about: Vec::new(),
            offered,
            routes,
            at: 0,
            saying: None,
            escape,
            reads: false,
        }
    }

    /// Sets what stands on the card's far edge, and what a reader needs to answer
    /// under the question.
    pub fn standing_on(mut self, edge: Option<String>, about: Vec<String>) -> Self {
        self.edge = edge;
        self.about = about;
        self
    }

    /// Offers the plan the decision is on, a key away.
    pub(crate) fn reading(mut self) -> Self {
        self.reads = true;
        self
    }

    /// What Escape does at the card.
    pub(crate) fn escape(&self) -> Escape {
        self.escape
    }

    /// Every option, as the desk lists them.
    pub(crate) fn rows(&self) -> Vec<ChoiceRow> {
        self.offered
            .iter()
            .map(|offered| offered.row.clone())
            .collect()
    }

    /// The option at `index`, chosen somewhere other than the card: what it comes to, or
    /// `None` where it asks for words, with the pointer on it and its field open.
    pub(crate) fn pick(&mut self, index: usize) -> Option<PickedOption> {
        let pointed = self.offered.get(index)?;
        if pointed.asks.is_none() {
            return Some(PickedOption {
                option: pointed.id.clone(),
                words: None,
                folded: false,
            });
        }
        self.at = index;
        self.open_field();
        None
    }

    /// The option the pointer stands on, or `None` when the card offers nothing.
    fn pointed(&self) -> Option<&Offered> {
        self.offered.get(self.at)
    }

    /// The body drawn while the list is open: every option, the pointer on one, and
    /// what follows that one when something does.
    fn choosing_body(&self, look: &Look) -> CardBody {
        CardBody::Choosing {
            choices: Choices {
                rows: self.rows(),
                at: self.at,
            },
            then: self
                .pointed()
                .and_then(|pointed| self.routes.get(&pointed.id))
                .map(|route| route.said(look.glyphs)),
        }
    }

    /// The body drawn while words are typed for the option pointed at. With nothing
    /// pointed at there is nothing to type for, so the list is drawn.
    fn typing_body(&self, saying: &Saying, look: &Look) -> CardBody {
        let Some(pointed) = self.pointed() else {
            return self.choosing_body(look);
        };
        card::saying(
            pointed.row.clone(),
            pointed.asks.clone(),
            saying.field.split(),
            saying.refused,
        )
    }

    /// The keys the card answers to, each with what it does, for the body it draws.
    fn legend(&self) -> Legend {
        match &self.saying {
            None => card::choosing_legend(self.reads, self.escape),
            Some(_) => {
                card::saying_legend(self.pointed().is_some_and(|pointed| pointed.asks.is_some()))
            }
        }
    }

    /// Moves the pointer one row down or up, around the list.
    fn point(&mut self, down: bool) {
        let rows = self.offered.len().max(1);
        self.at = match down {
            true => (self.at + 1) % rows,
            false => (self.at + rows - 1) % rows,
        };
    }

    /// Opens an empty field under the option pointed at. Words typed and left are
    /// not kept for the next time it opens.
    fn open_field(&mut self) {
        self.saying = Some(Saying {
            field: TypedLine::default(),
            refused: false,
        });
    }

    /// What a key asks of the list, with no field open: the pointer moves, Enter
    /// takes the option pointed at or opens its field, `n` opens a field for a note,
    /// `r` asks for the plan when the card offers it, and Escape or Ctrl-C leaves.
    fn press_in_list(&mut self, stroke: Stroke) -> Press<Reply> {
        match stroke {
            Stroke::Up | Stroke::BackTab | Stroke::Insert('k') => {
                self.point(false);
                Press::Stay
            }
            Stroke::Down | Stroke::Tab | Stroke::Insert('j') => {
                self.point(true);
                Press::Stay
            }
            Stroke::Enter => {
                let Some(pointed) = self.pointed() else {
                    return Press::Stay;
                };
                if pointed.asks.is_some() {
                    self.open_field();
                    return Press::Stay;
                }
                Press::Answer(Reply::Chose(PickedOption {
                    option: pointed.id.clone(),
                    words: None,
                    folded: false,
                }))
            }
            Stroke::Insert('n' | 'N') => {
                self.open_field();
                Press::Stay
            }
            Stroke::Insert('r' | 'R') if self.reads => Press::Answer(Reply::Read),
            Stroke::Decline => Press::Decline,
            Stroke::Interrupt => Press::Interrupt,
            _ => Press::Stay,
        }
    }
}

impl Answerable for DecisionCard {
    type Answer = Reply;

    /// Whether words are being typed for an option, so the prompt takes keys as text
    /// rather than as keys of the list.
    fn typing(&self) -> bool {
        self.saying.is_some()
    }

    /// The card as it stands: the list of options while no field is open, or the
    /// words being typed for the option pointed at.
    fn block(&self, look: &Look) -> Block {
        let body = match &self.saying {
            None => self.choosing_body(look),
            Some(saying) => self.typing_body(saying, look),
        };
        Block::Card(Card {
            question: self.question.clone(),
            edge: self.edge.clone(),
            about: self.about.clone(),
            body,
            legend: self.legend(),
        })
    }

    /// What a stroke asks of the card. With a field open, the stroke edits the words
    /// and an answer is refused while an option that asks for words has none; with
    /// the list open, the stroke moves the pointer or answers.
    fn press(&mut self, stroke: Stroke) -> Press<Reply> {
        let Some(saying) = self.saying.as_mut() else {
            return self.press_in_list(stroke);
        };
        match saying.field.apply(stroke) {
            Edit::Editing => {
                saying.refused = false;
                Press::Stay
            }
            Edit::Answered(typed) => {
                let Some(pointed) = self.offered.get(self.at) else {
                    return Press::Stay;
                };
                if pointed.asks.is_some() && typed.value.trim().is_empty() {
                    saying.refused = true;
                    return Press::Stay;
                }
                Press::Answer(Reply::Chose(PickedOption {
                    option: pointed.id.clone(),
                    words: (!typed.value.is_empty()).then_some(typed.value),
                    folded: typed.folded,
                }))
            }
            Edit::Declined => {
                self.saying = None;
                Press::Stay
            }
            Edit::Interrupted => Press::Interrupt,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
