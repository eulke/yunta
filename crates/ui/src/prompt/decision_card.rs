//! A decision a person answers with keys, as a card: the question, what a reader
//! needs to answer, every option with what it does and what it costs, one pointed
//! at, and what follows it. Pure: a stroke goes in, and what it asks of the prompt
//! comes out. No option names a command: the prompt is how one is chosen, and the
//! commands are said where the run is parked.
//!
//! The pointer moves by the arrows, `j k` and `tab`, and a number picks the option it
//! counts to, from the first; `enter` takes the option pointed at. A screen that holds
//! the card beside keys of its own, as a run's screen keeps the arrows for what stands
//! above it, hands it only the strokes it takes (`DecisionCard::takes`), so a key means
//! the same on every interface where it means anything to the card.

use surco::components::{Card, CardBody, ChoiceRow, Choices, Legend};
use surco::doc::Block;
use surco::look::Look;
use yunta_core::OptionId;
use yunta_render::prompt::card::{self, ChosenWith};
use yunta_render::prompt::Escape;

use super::{Answerable, DecisionPut, Edit, Press, Stroke, Typed, TypedLine};

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

/// One option of a decision as a prompt holds it, its card or the plan's desk: what it is
/// chosen by, its row, and what it asks of words where it needs them.
#[derive(Debug, Clone)]
pub(crate) struct Offered {
    pub(super) id: OptionId,
    pub(super) row: ChoiceRow,
    /// What it asks for, when it needs words.
    pub(super) asks: Option<String>,
}

impl Offered {
    /// It chosen with no words.
    pub(super) fn chosen(&self) -> PickedOption {
        PickedOption {
            option: self.id.clone(),
            words: None,
            folded: false,
        }
    }

    /// It chosen with the words `typed`, an empty line being no words: none where it asks
    /// for words and the line holds none, which refuses them.
    pub(super) fn said_with(&self, typed: Typed) -> Option<PickedOption> {
        if self.asks.is_some() && typed.value.trim().is_empty() {
            return None;
        }
        Some(PickedOption {
            option: self.id.clone(),
            words: (!typed.value.is_empty()).then_some(typed.value),
            folded: typed.folded,
        })
    }
}

/// The words an option is being said with.
#[derive(Debug, Clone)]
struct Saying {
    field: TypedLine,
    refused: bool,
}

/// A decision being answered.
#[derive(Debug, Clone)]
pub struct DecisionCard {
    /// The decision the card puts: what stands on its edge and under its question is
    /// said from it, in the glyphs the card is drawn with.
    put: DecisionPut,
    question: String,
    offered: Vec<Offered>,
    at: usize,
    saying: Option<Saying>,
    escape: Escape,
    /// Whether the plan the decision is on can be read from here, a key away.
    reads: bool,
}

impl DecisionCard {
    /// The card for the decision `put`: its question, and every option it offers with
    /// what it says and what it costs, the pointer on the first, with what follows each
    /// option that leads somewhere. Escape at the card does what `escape` says.
    pub(crate) fn new(put: DecisionPut, escape: Escape) -> Self {
        let escalation = &put.escalation;
        let offered = escalation
            .options()
            .iter()
            .zip(yunta_render::offers::decision_options(
                escalation, &put.ahead,
            ))
            .map(|(option, words)| {
                let said = words.said_label().map(str::to_string);
                Offered {
                    id: option.id.clone(),
                    row: ChoiceRow {
                        name: words.id,
                        said,
                        cost: words.tradeoff,
                        key: None,
                    },
                    asks: words.asks,
                }
            })
            .collect();
        Self {
            question: yunta_render::escalation::question(escalation),
            offered,
            put,
            at: 0,
            saying: None,
            escape,
            reads: false,
        }
    }

    /// Offers the plan the decision is on, a key away.
    pub(crate) fn reading(mut self) -> Self {
        self.reads = true;
        self
    }

    /// It without `option`, where another key does what that option does and it offers
    /// another: a card in place on a scope asked for leaves pausing to `esc`, which leaves
    /// the decision open alike.
    pub(crate) fn leaving_out(mut self, option: &OptionId) -> Self {
        let others = self.offered.iter().any(|offered| offered.id != *option);
        if others {
            self.offered.retain(|offered| offered.id != *option);
            self.at = self.at.min(self.offered.len() - 1);
        }
        self
    }

    /// The decision the card puts.
    pub(crate) fn decision(&self) -> &DecisionPut {
        &self.put
    }

    /// What Escape does at the card.
    pub(crate) fn escape(&self) -> Escape {
        self.escape
    }

    /// Every option, as the card draws them.
    fn rows(&self) -> Vec<ChoiceRow> {
        self.offered
            .iter()
            .map(|offered| offered.row.clone())
            .collect()
    }

    /// Every option it offers, as the plan's desk offers them too.
    pub(crate) fn offered(&self) -> Vec<Offered> {
        self.offered.clone()
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
                .and_then(|pointed| self.put.routes.get(&pointed.id))
                .map(|route| route.said(look.glyphs)),
        }
    }

    /// The body drawn while words are typed for the option pointed at: every option, and
    /// under them the line the words are typed on. With nothing pointed at there is nothing
    /// to type for, so the list is drawn.
    fn typing_body(&self, saying: &Saying, look: &Look) -> CardBody {
        let Some(pointed) = self.pointed() else {
            return self.choosing_body(look);
        };
        let choices = Choices {
            rows: self.rows(),
            at: self.at,
        };
        card::saying(
            choices,
            pointed.asks.clone(),
            saying.field.split(),
            saying.refused,
        )
    }

    /// The keys the card answers to, each with what it does, for the body it draws.
    fn legend(&self) -> Legend {
        match &self.saying {
            None => card::choosing_legend(ChosenWith::Arrows, self.reads, self.escape.does()),
            Some(_) => card::saying_legend(self.words_asked()),
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

    /// What `stroke` means to the list, with no field open: the pointer moved up or
    /// down, an option picked by the number it counts to from the first, the option
    /// pointed at taken, a note, the plan asked for when the card offers it, leaving or
    /// stopping. None for a key the list has no use for.
    fn list_key(&self, stroke: Stroke) -> Option<ListKey> {
        match stroke {
            Stroke::Up | Stroke::BackTab | Stroke::Insert('k') => Some(ListKey::Up),
            Stroke::Down | Stroke::Tab | Stroke::Insert('j') => Some(ListKey::Down),
            Stroke::Insert(digit @ '1'..='9') => {
                let index = usize::from(digit as u8 - b'1');
                (index < self.offered.len()).then_some(ListKey::Pick(index))
            }
            Stroke::Enter => Some(ListKey::Take),
            Stroke::Insert('n' | 'N') => Some(ListKey::Note),
            Stroke::Insert('r' | 'R') if self.reads => Some(ListKey::Read),
            Stroke::Decline => Some(ListKey::Leave),
            Stroke::Interrupt => Some(ListKey::Stop),
            _ => None,
        }
    }

    /// What a key asks of the list, with no field open: the pointer moves, a number
    /// picks the option it counts to, Enter takes the option pointed at or opens its
    /// field, `n` opens a field for a note, `r` asks for the plan when the card offers
    /// it, and Escape or Ctrl-C leaves.
    fn press_in_list(&mut self, stroke: Stroke) -> Press<Reply> {
        let Some(key) = self.list_key(stroke) else {
            return Press::Stay;
        };
        match key {
            ListKey::Up => {
                self.point(false);
                Press::Stay
            }
            ListKey::Down => {
                self.point(true);
                Press::Stay
            }
            ListKey::Pick(index) => {
                self.at = index;
                Press::Stay
            }
            ListKey::Take => {
                let Some(pointed) = self.pointed() else {
                    return Press::Stay;
                };
                if pointed.asks.is_some() {
                    self.open_field();
                    return Press::Stay;
                }
                Press::Answer(Reply::Chose(pointed.chosen()))
            }
            ListKey::Note => {
                self.open_field();
                Press::Stay
            }
            ListKey::Read => Press::Answer(Reply::Read),
            ListKey::Leave => Press::Decline,
            ListKey::Stop => Press::Interrupt,
        }
    }

    /// Whether `stroke` means anything to the card as it stands: every stroke while
    /// words are typed for an option, else each key its list answers to. What a screen
    /// that holds the card beside keys of its own hands it, keeping the rest.
    pub(crate) fn takes(&self, stroke: Stroke) -> bool {
        self.saying.is_some() || self.list_key(stroke).is_some()
    }

    /// Whether the card offers the plan its decision is on, a key away.
    pub(crate) fn offers_the_plan(&self) -> bool {
        self.reads
    }

    /// Whether the option pointed at asks for words of its own, which `enter` sends with
    /// it; else what is typed for it is a note.
    pub(crate) fn words_asked(&self) -> bool {
        self.pointed().is_some_and(|pointed| pointed.asks.is_some())
    }
}

/// What a key means to a card's list, with no field open. Not a [`Stroke`], what a
/// surface read: several strokes mean one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKey {
    /// The pointer one row up, around the list.
    Up,
    /// The pointer one row down, around the list.
    Down,
    /// The pointer on the option at this place among them, counted from naught.
    Pick(usize),
    /// The option pointed at taken, or its field opened where it asks for words.
    Take,
    /// A field opened for a note on the option pointed at.
    Note,
    /// The plan the decision is on asked for.
    Read,
    /// The card left, nothing chosen.
    Leave,
    /// The run stopped.
    Stop,
}

impl Answerable for DecisionCard {
    type Answer = Reply;

    /// Whether words are being typed for an option, so the prompt takes keys as text
    /// rather than as keys of the list.
    fn typing(&self) -> bool {
        self.saying.is_some()
    }

    /// The card as it stands: the list of options, and under it, while a field is open,
    /// the words being typed for the option pointed at.
    fn block(&self, look: &Look) -> Block {
        let body = match &self.saying {
            None => self.choosing_body(look),
            Some(saying) => self.typing_body(saying, look),
        };
        let put = &self.put;
        Block::Card(Card {
            question: self.question.clone(),
            edge: card::edge(put.node.as_ref(), put.run()),
            about: card::about(
                &put.escalation,
                put.documents(),
                put.changed.as_ref(),
                look.glyphs,
            ),
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
                match pointed.said_with(typed) {
                    Some(picked) => Press::Answer(Reply::Chose(picked)),
                    None => {
                        saying.refused = true;
                        Press::Stay
                    }
                }
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
