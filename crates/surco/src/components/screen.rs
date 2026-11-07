//! What a whole interface shows, as data: where the person is, what the
//! screen is about, its regions, what needs the person and the keys, with no
//! cell, no pixel and no color.
//!
//! Each interface lays a screen out its own way: a terminal in rows and
//! columns, a desktop in boxes that flex, a page in a grid. Each draws the
//! documents a screen holds with its own surface, a terminal's pane at
//! [`Width::exact`](crate::look::Width::exact). A screen keeps no scroll
//! position, width or size: an interface keeps where a person scrolled to and
//! how wide each pane stands. Its parts are named by what they are, never by
//! where they stand, so two interfaces that place them apart still show the
//! same screen.

use super::{Attention, Legend};
use crate::doc::Doc;
use crate::words::{Standing, NEED_YOU};

/// What a whole interface shows, with no cell, no pixel and no color. Not a
/// [`Doc`]: a screen holds documents, each in its place.
pub struct Screen {
    /// Where the person is, from the outermost: `yunta`, a run, a node.
    pub trail: Vec<String>,
    /// How many runs stand at each word that asks for a look.
    pub tally: Vec<Count>,
    /// What the screen is about, across it: a run's headline, ribbon and gist.
    pub about: Option<Doc>,
    /// The region the screen is for.
    pub main: Region,
    /// A list beside the main region to move through, the first to give way.
    pub side: Option<Side>,
    pub attention: Option<Attention>,
    /// The keys of what is on screen.
    pub legend: Legend,
    /// What is open over the regions; its legend stands for the screen's
    /// while it is open.
    pub overlay: Option<Overlay>,
}

/// How many runs stand at one word: `13 need you`.
pub struct Count {
    pub standing: Standing,
    pub runs: usize,
}

/// A part of a screen that holds a document, under its title.
pub struct Region {
    pub title: Option<String>,
    pub doc: Doc,
}

/// The region beside the main one, and whether the keys act on it rather
/// than on the main one.
pub struct Side {
    pub region: Region,
    pub focused: bool,
}

/// What opens over a screen and closes with `esc`: the palette, help, a
/// confirmation.
pub struct Overlay {
    pub title: String,
    pub doc: Doc,
    pub legend: Legend,
}

impl Screen {
    /// A screen for `main`, the keys on it, with nothing else on it yet.
    pub fn new(main: Region) -> Self {
        Screen {
            trail: Vec::new(),
            tally: Vec::new(),
            about: None,
            main,
            side: None,
            attention: None,
            legend: Legend::new(),
            overlay: None,
        }
    }

    /// This screen with the person at `places`, from the outermost.
    pub fn trail<S: Into<String>>(mut self, places: impl IntoIterator<Item = S>) -> Self {
        self.trail = places.into_iter().map(Into::into).collect();
        self
    }

    /// This screen counting `runs` at `standing`, after what it counts so far.
    pub fn tally(mut self, standing: Standing, runs: usize) -> Self {
        self.tally.push(Count { standing, runs });
        self
    }

    /// This screen about what `doc` says, across it.
    pub fn about(mut self, doc: Doc) -> Self {
        self.about = Some(doc);
        self
    }

    /// This screen with `region` beside the main one, the keys still on the
    /// main one.
    pub fn side(mut self, region: Region) -> Self {
        self.side = Some(Side {
            region,
            focused: false,
        });
        self
    }

    /// This screen with the keys on its side region; with no side region,
    /// they stay on the main one.
    pub fn focus_side(mut self) -> Self {
        if let Some(side) = &mut self.side {
            side.focused = true;
        }
        self
    }

    /// This screen with `attention` asking for the person.
    pub fn attention(mut self, attention: Attention) -> Self {
        self.attention = Some(attention);
        self
    }

    /// This screen with `legend` for its keys.
    pub fn legend(mut self, legend: Legend) -> Self {
        self.legend = legend;
        self
    }

    /// This screen with `overlay` open over its regions.
    pub fn overlay(mut self, overlay: Overlay) -> Self {
        self.overlay = Some(overlay);
        self
    }

    /// Whether the keys act on the main region: they do unless the side
    /// region has them.
    pub fn main_focused(&self) -> bool {
        !self.side.as_ref().is_some_and(|side| side.focused)
    }
}

impl Count {
    /// The count and its word, `needs you` agreeing with it: `1 needs you`,
    /// `13 need you`, `2 stalled`.
    pub fn said(&self) -> String {
        match (self.standing, self.runs) {
            (Standing::NeedsYou, runs) if runs != 1 => format!("{runs} {NEED_YOU}"),
            (standing, runs) => format!("{runs} {}", standing.word()),
        }
    }
}

impl Region {
    /// A region that holds `doc`, with no title.
    pub fn new(doc: Doc) -> Self {
        Region { title: None, doc }
    }

    /// This region under `title`.
    pub fn titled(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl Overlay {
    /// An overlay under `title` that holds `doc`, with no keys yet.
    pub fn new(title: impl Into<String>, doc: Doc) -> Self {
        Overlay {
            title: title.into(),
            doc,
            legend: Legend::new(),
        }
    }

    /// This overlay with `legend` for its keys.
    pub fn legend(mut self, legend: Legend) -> Self {
        self.legend = legend;
        self
    }
}

#[cfg(test)]
mod tests;
