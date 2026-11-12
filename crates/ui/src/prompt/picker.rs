//! The one list a person picks from.
//!
//! Every list a prompt has is this one: the values a `choice` question
//! declares, the two a `boolean` question has, the shapes `yunta new` offers, and
//! so on. Arrows move through it, typing narrows it to what matches, Enter takes
//! what is pointed at and Escape leaves without an answer. A row has no number: a
//! digit narrows the list as any letter does, so a value with digits in it is typed
//! as written. A decision is not this list: its letters are keys.

use surco::components::{ChoiceRow, Choices};
use surco::doc::Block;
use surco::look::Look;
use yunta_render::prompt::Escape;

use super::{Answerable, Press, Stroke};

/// One of the alternatives a list offers: what it is called, what it does when its
/// name does not say it, and what the caller gets back for it. Not core's
/// `step::Choice`, how a command line chooses on a decision.
#[derive(Debug, Clone)]
pub struct Alternative<T> {
    pub name: String,
    pub label: Option<String>,
    pub value: T,
}

impl<T> Alternative<T> {
    /// An alternative its name says all of.
    pub fn named(name: impl Into<String>, value: T) -> Self {
        Alternative {
            name: name.into(),
            label: None,
            value,
        }
    }
}

/// A list being answered.
#[derive(Debug, Clone)]
pub struct Picker<T> {
    choices: Vec<Alternative<T>>,
    /// What was typed to narrow it.
    filter: String,
    /// Where the pointer stands among what the filter leaves.
    at: usize,
    escape: Escape,
}

impl<T> Picker<T> {
    /// A list of `choices` with nothing typed and the pointer on the first of them.
    pub fn new(choices: Vec<Alternative<T>>, escape: Escape) -> Self {
        Self {
            choices,
            filter: String::new(),
            at: 0,
            escape,
        }
    }

    /// The indices of the options the filter leaves: the best answers to what was
    /// typed first, and in the order they were given among equals.
    fn matching(&self) -> Vec<usize> {
        let typed = self.filter.to_lowercase();
        let mut found: Vec<(u8, usize)> = self
            .choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| {
                Some((answers(&choice.name.to_lowercase(), &typed)?, index))
            })
            .collect();
        found.sort_unstable();
        found.into_iter().map(|(_, index)| index).collect()
    }

    /// The index of the option the pointer is on, or `None` when the filter leaves
    /// nothing.
    fn pointed(&self) -> Option<usize> {
        self.matching().get(self.at).copied()
    }

    /// What the filter leaves as a list to draw, or `None` when it leaves nothing.
    pub(crate) fn list(&self) -> Option<Choices> {
        let matching = self.matching();
        if matching.is_empty() {
            return None;
        }
        Some(Choices {
            rows: matching
                .iter()
                .filter_map(|&index| self.choices.get(index))
                .map(|choice| ChoiceRow {
                    name: choice.name.clone(),
                    said: choice.label.clone(),
                    cost: None,
                })
                .collect(),
            at: self.at.min(matching.len() - 1),
        })
    }

    /// What was typed to narrow the list, when anything was.
    pub(crate) fn filter(&self) -> Option<&str> {
        (!self.filter.is_empty()).then_some(self.filter.as_str())
    }

    /// The first of every option, whatever was typed: what Escape takes on a list whose
    /// Escape takes the first, the plainest.
    pub fn first(&self) -> Option<T>
    where
        T: Clone,
    {
        self.choices.first().map(|choice| choice.value.clone())
    }

    /// This list with its pointer on the option at `index`, among all of them.
    pub(crate) fn pointing_at(mut self, index: usize) -> Self {
        self.at = index;
        self
    }

    /// Moves the pointer by one row within what the filter leaves, around at its ends.
    fn move_by(&mut self, forward: bool) {
        let rows = self.matching().len().max(1);
        self.at = match forward {
            true => (self.at + 1) % rows,
            false => (self.at + rows - 1) % rows,
        };
    }

    /// Takes the option the pointer is on, or stays when the filter leaves nothing. The
    /// list keeps every option where it stood: a picker a person comes back to after
    /// answering it lists what it listed.
    fn answer(&self) -> Press<T>
    where
        T: Clone,
    {
        match self.pointed() {
            Some(index) => Press::Answer(self.choices[index].value.clone()),
            None => Press::Stay,
        }
    }
}

/// How well `name` answers what was typed, the lower the better: the whole of it, its
/// start, a run of it, or its letters in order with anything between them. `None` where
/// it holds none of those. Both are lowercase.
fn answers(name: &str, typed: &str) -> Option<u8> {
    match () {
        () if name == typed => Some(0),
        () if name.starts_with(typed) => Some(1),
        () if name.contains(typed) => Some(2),
        () if holds_in_order(name, typed) => Some(3),
        () => None,
    }
}

/// Whether `name` holds the letters of `typed` in order, whatever stands between them.
fn holds_in_order(name: &str, typed: &str) -> bool {
    let mut wanted = typed.chars().peekable();
    for character in name.chars() {
        if wanted.peek() == Some(&character) {
            wanted.next();
        }
    }
    wanted.peek().is_none()
}

impl<T: Clone> Answerable for Picker<T> {
    type Answer = T;

    /// The list as a block: what the filter leaves, what was typed, and the keys it
    /// answers to with what Escape does here.
    fn block(&self, _look: &Look) -> Block {
        Block::ChoiceList(yunta_render::prompt::choice_list(
            self.list(),
            self.filter(),
            self.escape,
        ))
    }

    fn press(&mut self, stroke: Stroke) -> Press<T> {
        match stroke {
            Stroke::Up | Stroke::BackTab => self.move_by(false),
            Stroke::Down | Stroke::Tab => self.move_by(true),
            Stroke::Insert(character) => {
                self.filter.push(character);
                self.at = 0;
            }
            Stroke::Backspace => {
                self.filter.pop();
                self.at = 0;
            }
            Stroke::Enter => return self.answer(),
            Stroke::Decline => return Press::Decline,
            Stroke::Interrupt => return Press::Interrupt,
            _ => {}
        }
        Press::Stay
    }

    fn typing(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;
