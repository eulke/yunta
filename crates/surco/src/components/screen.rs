//! What a whole interface shows, as data: where the person is, what the
//! screen is about, its regions, what needs the person and the keys, with no
//! cell, no pixel and no color.
//!
//! Each interface lays a screen out its own way: a terminal in rows and
//! columns, a desktop in boxes that flex, a page in a grid. Each draws the
//! documents a screen holds with its own surface, a terminal's pane at
//! [`Width::exact`](crate::look::Width::exact). A screen keeps no width or
//! size: an interface keeps how wide each pane stands. What of each region a
//! person asked to see is the region's [`Sight`], kept by whoever answers their
//! keys; an interface that shows part of a region hands back what it laid out of
//! each, a [`ScreenLaid`], for the next key to move from. Its parts are named by
//! what they are, never by where they stand, so two interfaces that place them
//! apart still show the same screen.
//!
//! A screen that stands among runs counts them in its tally, at each word that asks for a look,
//! and says that nothing needs the person where none stands at one; a screen that follows no
//! runs, as the signpost outside a repository, counts nothing and says nothing of them. The
//! two are told apart by [`Tally`], never by an empty list.
//!
//! A screen about a run says, under its regions, what the run has spent: its meter, which
//! a terminal draws on the row over the attention row's place.
//!
//! A screen's keys are two legends: its own, the keys of what is on it, and the far keys,
//! pressed anywhere a person is not typing (the palette, and help, which lists every key).
//! A terminal wide enough draws the far keys at the far end of the keys' row; a narrower one
//! keeps help's key alone, last after the screen's. Where a line is typed every key types, so
//! there are no far keys and no key that lists every key, and over a box there are none: its
//! own keys stand for the screen's.

use super::{Attention, Laid, Legend, Meter, Sight};
use crate::doc::{Block, Doc};
use crate::glyphs::Mark;
use crate::words::{Standing, NEED_YOU};

/// What a whole interface shows, with no cell, no pixel and no color. Not a
/// [`Doc`]: a screen holds documents, each in its place.
pub struct Screen {
    /// Where the person is, from the outermost: `yunta`, a run, a node.
    pub trail: Vec<String>,
    /// How many runs stand at each word that asks for a look, where the screen counts them.
    pub tally: Tally,
    /// What the screen is about, across it: a run's headline, ribbon and gist.
    pub about: Option<Doc>,
    /// The region the screen is for.
    pub main: Region,
    /// A list beside the main region to move through, the first to give way.
    pub side: Option<Side>,
    /// What the run the screen is about has spent, on the row over the attention row's
    /// place: none on a screen about no run.
    pub meter: Option<Meter>,
    pub attention: Option<Attention>,
    /// What the person's last act came to. It stands in the attention row's place until
    /// the next key, and news leave it.
    pub noted: Option<Noted>,
    /// The keys of what is on screen.
    pub legend: Legend,
    /// The keys a person presses anywhere they are not typing, the palette's and help's, help's
    /// last: it lists every key, so a row too narrow for them all keeps it alone. Not
    /// [`legend`](Self::legend), the keys of what is on this screen. None where a line is typed.
    pub anywhere: Legend,
    /// Whether a line is typed on the screen itself: every printable key types there, so it
    /// has no far keys and none of its keys lists every key, and a row too narrow for its
    /// keys keeps the first that fit and says that more stand after them. Not a box over the
    /// screen whose line is typed, which brings its own keys.
    pub typed: bool,
    /// What is open over the regions; its legend stands for the screen's
    /// while it is open.
    pub overlay: Option<Overlay>,
    /// The window's title while a run needs the person; none leaves the
    /// window its own. Only a terminal shows it, as its window's title:
    /// Markdown and HTML draw none of it. Not a region's title.
    pub title: Option<String>,
}

/// What an act came to, said once until the next key: a mark and a sentence,
/// `✓ Recorded: approve.` Not a [`Note`](super::Note), a muted paragraph inside a
/// document.
pub struct Noted {
    pub mark: Mark,
    /// What the act did, as a sentence.
    pub said: String,
}

/// What a screen counts of the runs it stands among. Not [`Count`], how many runs stand at
/// one word.
pub enum Tally {
    /// The screen counts no runs, as one that follows nothing: it says nothing of them.
    Uncounted,
    /// The runs counted at each word that asks for a look, a word no run stands at left out.
    /// With none, the screen says that nothing needs the person.
    Counted(Vec<Count>),
}

/// How many runs stand at one word: `13 need you`.
pub struct Count {
    pub standing: Standing,
    pub runs: usize,
}

/// A part of a screen that holds a document, under its title, and what of it a
/// person asked to see.
pub struct Region {
    /// What the region is, in parts a separator stands between, as a desk's title is: the
    /// first names it, the rest say of what kind (`plan · prompt`). Empty for a region with no
    /// title.
    pub title: Vec<String>,
    /// The words at the far end of its title row: who works a node, its task, its check, its
    /// assignee. Said only where the region has a title.
    pub edge: Option<String>,
    pub doc: Doc,
    /// What stands above the feed its document ends with, said again in fewer rows, each in
    /// fewer than the one before, the heading the feed hangs from left out of them all, since
    /// it stays. A pane whose document leaves the feed's window fewer rows than it keeps at
    /// least draws the first of these that leaves them; the last is what the region says at
    /// the least, which a pane that stands close keeps whole, giving the feed what is left.
    /// Empty where what stands above a feed gives way from its top, a row at a time. In a
    /// region holding a card, what stands above the card said again in fewer rows: a pane
    /// that leaves it fewer rows than it needs draws the first of these that fits. In a
    /// region with no feed and no prompt, its whole document said again in fewer rows: a
    /// pane too short for all of it draws the first of these that fits. A surface that draws
    /// a region whole says its document. Not the region's [`Sight`], what of it a person
    /// asked to see.
    pub abridged: Vec<Vec<Block>>,
    pub sight: Sight,
}

/// What a surface laid out of a screen's regions: of the main one, of the one beside it
/// where it stands, and of what is open over them where something is. A region holding
/// a desk is laid out as its pane, and a region whose document ends with a feed as its
/// feed: counted from the whole feed's first row, its page the feed's rows in sight at its
/// end, whatever stands above the feed in the region.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenLaid {
    pub main: Laid,
    pub side: Option<Laid>,
    pub overlay: Option<Laid>,
}

/// The region beside the main one, the edge it stands at, and whether the keys act on it
/// rather than on the main one.
pub struct Side {
    pub region: Region,
    pub at: SideAt,
    pub focused: bool,
}

/// The edge of a screen the region beside the main one stands at, where an interface lays
/// its regions side by side: the left for a list a person moves along, as a run's nodes;
/// the right for what is read beside the main region, as the field's glance. An interface
/// that lays its regions in one column, as Markdown and HTML do, says the side region first
/// at either edge. Not [`Side`], the region itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideAt {
    Left,
    Right,
}

/// What opens over a screen and closes with `esc`: the palette, help, a
/// confirmation; and what of it a person asked to see.
pub struct Overlay {
    pub title: String,
    pub doc: Doc,
    pub legend: Legend,
    pub sight: Sight,
}

impl Screen {
    /// A screen for `main`, the keys on it, with nothing else on it yet.
    pub fn new(main: Region) -> Self {
        Screen {
            trail: Vec::new(),
            tally: Tally::Uncounted,
            about: None,
            main,
            side: None,
            meter: None,
            attention: None,
            noted: None,
            legend: Legend::new(),
            anywhere: Legend::new(),
            typed: false,
            overlay: None,
            title: None,
        }
    }

    /// This screen with the person at `places`, from the outermost.
    pub fn trail<S: Into<String>>(mut self, places: impl IntoIterator<Item = S>) -> Self {
        self.trail = places.into_iter().map(Into::into).collect();
        self
    }

    /// This screen counting `runs` at `standing`, after what it counts so far.
    pub fn tally(mut self, standing: Standing, runs: usize) -> Self {
        let count = Count { standing, runs };
        match &mut self.tally {
            Tally::Counted(counts) => counts.push(count),
            Tally::Uncounted => self.tally = Tally::Counted(vec![count]),
        }
        self
    }

    /// This screen about what `doc` says, across it.
    pub fn about(mut self, doc: Doc) -> Self {
        self.about = Some(doc);
        self
    }

    /// This screen with `region` beside the main one, at its edge `at`, the keys still on
    /// the main one.
    pub fn side(mut self, region: Region, at: SideAt) -> Self {
        self.side = Some(Side {
            region,
            at,
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

    /// This screen saying what the run it is about has spent, `meter`.
    pub fn meter(mut self, meter: Meter) -> Self {
        self.meter = Some(meter);
        self
    }

    /// This screen with `attention` asking for the person.
    pub fn attention(mut self, attention: Attention) -> Self {
        self.attention = Some(attention);
        self
    }

    /// This screen saying what the person's last act came to, in the attention row's
    /// place.
    pub fn noted(mut self, noted: Noted) -> Self {
        self.noted = Some(noted);
        self
    }

    /// This screen with `legend` for its keys.
    pub fn legend(mut self, legend: Legend) -> Self {
        self.legend = legend;
        self
    }

    /// This screen with `anywhere` for the keys a person presses anywhere they are not
    /// typing, help's last.
    pub fn anywhere(mut self, anywhere: Legend) -> Self {
        self.anywhere = anywhere;
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
    /// A region that holds `doc`, with no title, from its first line.
    pub fn new(doc: Doc) -> Self {
        Region {
            title: Vec::new(),
            edge: None,
            doc,
            abridged: Vec::new(),
            sight: Sight::default(),
        }
    }

    /// This region with what stands above its feed or its card, or with neither its whole
    /// document, said again in fewer rows, `abridged`, each in fewer than the one before.
    pub fn abridged(mut self, abridged: Vec<Vec<Block>>) -> Self {
        self.abridged = abridged;
        self
    }

    /// This region under a title of `parts`, the one that names it first.
    pub fn titled<S: Into<String>>(mut self, parts: impl IntoIterator<Item = S>) -> Self {
        self.title = parts.into_iter().map(Into::into).collect();
        self
    }

    /// This region with `edge` at the far end of its title row.
    pub fn edge(mut self, edge: impl Into<String>) -> Self {
        self.edge = Some(edge.into());
        self
    }

    /// This region with what of it `sight` asks to have in sight.
    pub fn in_sight(mut self, sight: Sight) -> Self {
        self.sight = sight;
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
            sight: Sight::default(),
        }
    }

    /// This overlay with what of it `sight` asks to have in sight.
    pub fn in_sight(mut self, sight: Sight) -> Self {
        self.sight = sight;
        self
    }

    /// This overlay with `legend` for its keys.
    pub fn legend(mut self, legend: Legend) -> Self {
        self.legend = legend;
        self
    }
}

#[cfg(test)]
mod tests;
