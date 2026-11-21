//! A region of a screen drawn in a pane: the one rule an engine of cells draws a region
//! by, so every surface and every test of one draws a region the same way. A region whose
//! document holds a prompt holds it, fitted to the rows the blocks around it leave, with
//! the cursor where it is typed into; any other region scrolls from what of it the person
//! asked to see. A pane is not a terminal's bottom row, so a prompt in one fills every row
//! it is given, where one held where a person is ([`Terminal::held`]) keeps the last.
//!
//! A card stands at the pane's foot under what it rests on (`card_at_the_foot`): whole,
//! taller than the shortest look, room made above it by the blank rows around what stands
//! above and by what stands above saying itself in fewer rows; at the shortest look keeping
//! only its options a row each, what stands above taking the rows they leave. What stands
//! above gives way from its top and scrolls from the place a person asked for. Where the
//! card's fewest rows do not stand under the region's title, the title gives way too, and the
//! pane takes its row ([`Terminal::keeps_its_title`]). A round of questions on a card gives
//! way by the same rule and stands under what it is about instead, at most a round's measure
//! wide. What stands above any other prompt is what it is about, kept in at most half of the
//! rows.
//!
//! A region that holds neither a prompt nor a feed and that says itself in fewer rows says
//! so where its rows are too few for all of it: the first of its abridgements that fits,
//! else all of it, scrolled (`fitted`).
//!
//! A region that holds neither and whose document ends with a horizon, the field in
//! perspective, lays its blocks out first, then the horizon in every row they leave, a blank
//! row parting them (`horizon`). Given fewer rows than the horizon's least, it draws none,
//! and the blocks stand where they stood; short of rows for the blocks themselves, their
//! blank rows give way before their rows scroll. The first request and a setup end with one,
//! and so does a run's receipt.
//!
//! A region whose document ends with a feed is laid out as its feed, as one holding a desk
//! is laid out as its pane: only the feed scrolls (`fed`). The blocks above it stand still
//! and take their rows first; the feed's window takes the rows they leave and fills down
//! from under them. Where the window would keep fewer than its least, what stands above
//! says itself in fewer rows where the region says how, else gives way from its top. At
//! its end the window shows the feed's newest rows, the newest the pane's last row, which
//! never gives way; scrolled back, the rows from the place asked for, the rows above and
//! under them counted.
//!
//! A titled region's pane opens a row under its title, a blank row parting them, and keeps
//! a blank row between two of its blocks. In the fewest rows a titled pane is given
//! ([`CLOSE_ROWS`], a node's pane at the least terminal) it stands close: no blank row under
//! its title nor between its blocks, each row one of what it says. Above a feed, what gives
//! way is counted with the blank rows standing, which then go, their rows the window's. A
//! prompt held in a pane keeps its own blank rows, which its drawing lets go in its own
//! order. On a screen standing short of rows, as on the least terminal, a page stands close
//! too ([`Terminal::pane_short_of_rows`]): an untitled region read from its top that holds
//! neither a prompt, a feed nor a horizon and says itself in no fewer rows lets its blank rows
//! go before its rows are counted under its window, where a taller screen keeps them and
//! counts the rows it hides; and a region holding a plan's desk holds it close, its own blank
//! rows given way. Whether a screen stands short of rows is its layout's to say.

use super::Terminal;
use crate::components::{Card, Laid, Region, Sight};
use crate::doc::{Block, Doc, Prompt};
use crate::line::Line;

mod card_at_the_foot;
mod fed;
mod fitted;
mod horizon;

use card_at_the_foot::Room;
use fed::Foot;

/// The most rows a pane stands close in, with no blank row: a node's pane under its title at
/// the least terminal, 60 by 14, where a row left blank is a row of what the node does lost.
const CLOSE_ROWS: usize = 5;

/// How a pane spaces what it holds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Spacing {
    /// A blank row under its title and between two of its blocks.
    Aired,
    /// No blank row, as a pane stands in its fewest rows.
    Close,
}

/// What a pane shows of a region: its lines, where the cursor stands when a prompt in it
/// is typed into (the row, counted from the pane's first, and the cell), and what was
/// laid out of what scrolls in it. Not [`Placed`](super::Placed): what a prompt held where
/// a person answers it draws.
pub struct Paned {
    pub lines: Vec<Line>,
    pub cursor: Option<(usize, usize)>,
    pub laid: Laid,
}

impl Terminal {
    /// `region` drawn in a pane `rows` high, at this look's width, which is the pane's. A
    /// region whose document holds a prompt draws the blocks under the prompt whole, the
    /// blocks above it in at most half of what those leave, giving way from their top, and
    /// holds the prompt in the rest; a prompt that cannot be asked in the rest takes the
    /// rows it needs from the blocks under it, which give way from their end. A card stands
    /// at the pane's foot instead, what stands above it taking every row its options leave
    /// and scrolling from its sight. A region whose
    /// document is a list of keys alone holds it from its sight, giving way as a list does.
    /// A region whose document ends with a feed shows the feed's window from its sight
    /// under the blocks above it, which give way from their top. Any other region scrolls
    /// from its sight. A titled region opens a row under its title; in the fewest rows a
    /// titled pane is given, a node's pane at the least terminal, it stands close, with no
    /// blank row.
    pub fn pane(&self, region: &Region, rows: usize) -> Paned {
        self.region_paned(region, rows, false)
    }

    /// `region` drawn in a pane `rows` high as [`Terminal::pane`] draws it, on a screen
    /// standing short of rows, as the least terminal's does: a page stands close there too,
    /// with no blank row, before what it hides is counted under its window.
    pub fn pane_short_of_rows(&self, region: &Region, rows: usize) -> Paned {
        self.region_paned(region, rows, true)
    }

    /// Whether `region`, drawn under its title in a pane `rows` high, keeps the title: it
    /// does, but where it holds a card whose fewest rows ([`Card::least`]) and what stands
    /// under it do not stand in those rows. The title is what stands above the card there,
    /// and gives way to it as the rest of what stands above it does, before the card gives
    /// way in its own order: the region is drawn in its title's row too
    /// ([`Terminal::pane_in_place_of_its_title`]).
    ///
    /// [`Card::least`]: crate::components::Card
    pub fn keeps_its_title(&self, region: &Region, rows: usize) -> bool {
        let blocks = region.doc.blocks();
        let Some(at) = region.doc.prompt_at() else {
            return true;
        };
        let Some(Prompt::Card(card)) = blocks[at].prompt() else {
            return true;
        };
        let below = parted(self.in_rhythm(&blocks[at + 1..]), Side::Below);
        card.least(&self.look) + below.len() <= rows
    }

    /// `region` drawn in a pane `rows` high that takes its title's row too, where it does not
    /// keep its title ([`Terminal::keeps_its_title`]): as a region with no title is drawn, on
    /// a screen standing `short_of_rows` where its layout says so.
    pub fn pane_in_place_of_its_title(
        &self,
        region: &Region,
        rows: usize,
        short_of_rows: bool,
    ) -> Paned {
        self.paned_titled(region, rows, short_of_rows, false)
    }

    /// `region` drawn in a pane `rows` high by the rule [`Terminal::pane`] states, a page
    /// standing close where the screen stands `short_of_rows`.
    fn region_paned(&self, region: &Region, rows: usize, short_of_rows: bool) -> Paned {
        self.paned_titled(region, rows, short_of_rows, !region.title.is_empty())
    }

    /// `region` drawn in a pane `rows` high by the rule [`Terminal::pane`] states, under its
    /// title where it stands `titled`, a page standing close where the screen stands
    /// `short_of_rows`.
    fn paned_titled(
        &self,
        region: &Region,
        rows: usize,
        short_of_rows: bool,
        titled: bool,
    ) -> Paned {
        let spacing = match titled {
            true if rows <= CLOSE_ROWS => Spacing::Close,
            false if short_of_rows && (a_page(region) || a_desk(region)) => Spacing::Close,
            true | false => Spacing::Aired,
        };
        let parted = titled && spacing == Spacing::Aired;
        let (doc, abridged) = (&region.doc, region.abridged.as_slice());
        if let Some((at, card)) = card_in(doc) {
            let room = Room {
                rows,
                under_a_title: parted,
                shortest: short_of_rows,
            };
            let blocks = (doc.blocks(), abridged);
            return self.card_at_the_foot(blocks, (at, card), region.sight, room, spacing);
        }
        if !parted {
            return self.spaced(doc, abridged, region.sight, rows, spacing);
        }
        let mut paned = self.spaced(doc, abridged, region.sight, rows - 1, spacing);
        paned.lines.insert(0, Line::new());
        paned.cursor = paned.cursor.map(|(row, cell)| (row + 1, cell));
        paned
    }

    /// `doc`, of which `sight` is what a person asked to see, drawn in a pane `rows` high
    /// by the rule [`Terminal::pane`] states, its blank rows standing: what is open over a
    /// screen is drawn by.
    pub(super) fn paned(&self, doc: &Doc, sight: Sight, rows: usize) -> Paned {
        self.spaced(doc, &[], sight, rows, Spacing::Aired)
    }

    /// `doc`, of which `sight` is what a person asked to see, drawn in a pane `rows` high
    /// by the rule [`Terminal::pane`] states, spaced as `spacing` says, what stands above a
    /// feed it ends with said in fewer rows as `abridged` says.
    fn spaced(
        &self,
        doc: &Doc,
        abridged: &[Vec<Block>],
        sight: Sight,
        rows: usize,
        spacing: Spacing,
    ) -> Paned {
        // A list of keys alone, as help is, gives way as a list does rather than scroll as
        // lines: its blank rows first, then its window counts the keys it hides.
        if let [Block::KeyList(list)] = doc.blocks() {
            let (lines, laid) = list.held(&self.look, sight, rows);
            return Paned {
                lines,
                cursor: None,
                laid,
            };
        }
        if let Some(foot) = Foot::of(doc) {
            return self.fed(&foot, abridged, sight, rows, spacing);
        }
        let Some(at) = doc.prompt_at() else {
            if let Some((Block::Horizon(horizon), above)) = doc.blocks().split_last() {
                return self.over_a_horizon(above, horizon, abridged, sight, rows, spacing);
            }
            let lines = self.fitted(doc.blocks(), abridged, rows, spacing);
            let (lines, laid) = self.scrolled_lines(&lines, sight, rows);
            return Paned {
                lines,
                cursor: None,
                laid,
            };
        };
        match doc.blocks()[at].prompt() {
            // A card outside a region, as what is open over a screen holds one, keeps its least
            // as a pane at the shortest look does.
            Some(Prompt::Card(card)) => {
                let room = Room {
                    rows,
                    under_a_title: false,
                    shortest: true,
                };
                self.card_at_the_foot((doc.blocks(), abridged), (at, card), sight, room, spacing)
            }
            Some(Prompt::Desk(_) | Prompt::TypedAnswer(_) | Prompt::ChoiceList(_)) | None => {
                self.prompted(doc.blocks(), at, rows, spacing)
            }
        }
    }

    /// `blocks`, whose block at `at` is a prompt other than a card, drawn in a pane `rows`
    /// high by the rule [`Terminal::pane`] states: what stands under the prompt whole, what
    /// stands above it in at most half of what that leaves, and the prompt in the rest; a
    /// plan's desk in a pane that stands close stands close itself.
    fn prompted(&self, blocks: &[Block], at: usize, rows: usize, spacing: Spacing) -> Paned {
        let below = parted(self.in_rhythm(&blocks[at + 1..]), Side::Below);
        let left = rows.saturating_sub(below.len());
        let mut above = parted(self.in_rhythm(&blocks[..at]), Side::Above);
        // What stands above a prompt is what it is about: it keeps the lines nearest the
        // prompt, and never more than half, so what a person answers stays in sight.
        let kept_above = above.len().min(left / 2);
        above.drain(..above.len() - kept_above);
        let held = match (blocks[at].prompt(), spacing) {
            (Some(Prompt::Desk(desk)), Spacing::Close) => {
                desk.drawn_close(&self.look, left - kept_above)
            }
            _ => self.held_in(&blocks[at], self.look.width.cells(), left - kept_above),
        };
        let cursor = held
            .cursor
            .map(|(row, cell)| (row + kept_above, cell))
            .filter(|(row, _)| *row < rows);
        // A prompt given fewer rows than it can be asked in still draws those: what stands
        // under it gives way from its end, since what a person answers, and the row they
        // type on, stay in sight.
        let mut lines = above;
        lines.extend(held.lines);
        lines.extend(below);
        lines.truncate(rows);
        Paned {
            lines,
            cursor,
            laid: held.laid.unwrap_or_default(),
        }
    }
}

/// The card `doc` holds and where it stands among its blocks, where it holds one.
fn card_in(doc: &Doc) -> Option<(usize, &Card)> {
    let at = doc.prompt_at()?;
    match doc.blocks()[at].prompt()? {
        Prompt::Card(card) => Some((at, card)),
        Prompt::Desk(_) | Prompt::TypedAnswer(_) | Prompt::ChoiceList(_) => None,
    }
}

/// Whether `region` holds a plan's desk, which lays out columns of its own across the pane.
fn a_desk(region: &Region) -> bool {
    let doc = &region.doc;
    let held = doc.prompt_at().and_then(|at| doc.blocks()[at].prompt());
    matches!(held, Some(Prompt::Desk(_)))
}

/// Whether `region` is a page: read from its top, no pointer to follow, saying itself in no
/// fewer rows, and holding neither a prompt, a feed at its foot, a horizon at its end nor a
/// list of keys alone, so it is drawn whole where it fits and scrolled where it does not.
fn a_page(region: &Region) -> bool {
    let doc = &region.doc;
    let drawn_otherwise = matches!(doc.blocks(), [Block::KeyList(_)])
        || matches!(doc.blocks().last(), Some(Block::Horizon(_)))
        || Foot::of(doc).is_some()
        || doc.prompt_at().is_some();
    !region.sight.follows_pointer && region.abridged.is_empty() && !drawn_otherwise
}

/// `lines` as a pane spaced by `spacing` holds them: whole where it is aired, with no blank
/// line where it stands close.
fn closed_up(mut lines: Vec<Line>, spacing: Spacing) -> Vec<Line> {
    if spacing == Spacing::Close {
        lines.retain(|line| !line.text().is_empty());
    }
    lines
}

/// Which side of a prompt blocks stand on.
enum Side {
    Above,
    Below,
}

/// `lines` with the blank row that parts them from a prompt on `side`; nothing for none.
fn parted(mut lines: Vec<Line>, side: Side) -> Vec<Line> {
    if lines.is_empty() {
        return lines;
    }
    match side {
        Side::Above => lines.push(Line::new()),
        Side::Below => lines.insert(0, Line::new()),
    }
    lines
}

#[cfg(test)]
mod tests;
