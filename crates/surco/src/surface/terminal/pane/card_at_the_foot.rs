//! See [`super`]. A region whose document holds a card, the decision a person answers there:
//! the card stands at the pane's foot, under what it rests on.
//!
//! What stands under the card stands whole. Taller than the shortest look, the card stands
//! whole too, its question and its options' words, and room is made above it: the blank row
//! parting what stands above from the card goes first; then what stands above says itself in
//! fewer rows where the region says how, the first of its abridgements that fits, as a
//! failure lets its attempt go and then the end of what it printed from its top, counted;
//! then the blank row under the region's title goes, where a title stands over the pane.
//! Where none of those fits, what stands above gives way from its top in the rows the card
//! leaves.
//!
//! At the shortest look the card keeps first only the rows it cannot do without
//! ([`Card::least`]): its question, what stands under it, and each option on a row of its own,
//! so no option ever gives way to what stands above it. What stands above takes the rows that
//! leaves, as many as it needs and no more, saying itself in fewer rows where the region says
//! how; the card takes every row left, giving way in its own order where they are fewer than
//! it needs whole.
//!
//! What stands above gives way from its top where the person reads it at its end, as a pane
//! opens on it, and scrolls from the place they asked for, the rows that leaves counted in
//! what was laid out, which the keys that scroll move from. The rows the card does not take
//! stand blank between it and what stands above it, so the card stands at the foot. Spaced
//! close, nothing parts what stands around the card from it, and what stands above keeps no
//! blank row.
//!
//! A round of questions on a card gives way by the same rule, but it is read from its first
//! question down under what it is about: the rows it does not take stand blank under it, and
//! it spans at most [`ROUND_MOST`] cells of a wider pane, with what stands around it.

use super::{closed_up, parted, Paned, Side, Spacing, Terminal};
use crate::components::{Card, CardBody, Laid, Sight};
use crate::doc::Block;
use crate::line::Line;

/// The most cells a round of questions spans in a pane, however wide the pane: a question
/// with the answer beside it, and the run's handle at the card's far edge, read in one sweep
/// at that measure, as the design sets a round on its widest screen.
const ROUND_MOST: usize = 110;

/// The rows a card's pane holds and how it stands: whether a blank row is owed under the
/// region's title, which the pane draws as its own first row, and whether the screen stands
/// at its shortest look, where the card keeps its least. Not [`Laid`], what a surface laid
/// out of it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Room {
    pub(super) rows: usize,
    pub(super) under_a_title: bool,
    pub(super) shortest: bool,
}

impl Room {
    /// The room of a card's pane `rows` high, under its title where it stands `titled`, spaced
    /// as `spacing` says, on a screen standing `short_of_rows` where its layout says so: a blank
    /// row is owed under the title where an aired pane stands under one, and the card keeps its
    /// least where the screen stands short of rows.
    pub(super) fn under_the_title(
        rows: usize,
        titled: bool,
        spacing: Spacing,
        short_of_rows: bool,
    ) -> Room {
        Room {
            rows,
            under_a_title: titled && spacing == Spacing::Aired,
            shortest: short_of_rows,
        }
    }
}

/// What stands above a card as the pane draws it: whether the blank row under the region's
/// title stands, the lines of what stands above, and the rows of them that stand.
struct Above {
    title_blank: bool,
    lines: Vec<Line>,
    page: usize,
}

/// A card's pane laid out by the rule this module states, before it is drawn: the terminal
/// measured for the card, what stands under it, what stands above it, and the rows the card
/// takes between them. What the pane draws and whether the card's least stands, read once.
struct Footing {
    on: Terminal,
    below: Vec<Line>,
    above: Above,
    card_rows: usize,
}

impl Terminal {
    /// `blocks`, a region's, whose block at `at` is `card`, of which `sight` is what a person
    /// asked to see above the card and `abridged` what stands above it said in fewer rows,
    /// drawn in the pane `room` says, spaced as `spacing` says, by the rule this module states.
    pub(super) fn card_at_the_foot(
        &self,
        (blocks, abridged): (&[Block], &[Vec<Block>]),
        (at, card): (usize, &Card),
        sight: Sight,
        room: Room,
        spacing: Spacing,
    ) -> Paned {
        let Footing {
            on,
            below,
            above,
            card_rows,
        } = self.footing((blocks, abridged), (at, card), room, spacing);
        let title_blank = usize::from(above.title_blank);
        let most = above.lines.len() - above.page;
        let top = sight.top.min(most);
        let held = on.held_in(&blocks[at], on.look.width.cells(), card_rows);
        let unheld = card_rows.saturating_sub(held.lines.len());
        // A decision's card stands at the pane's foot, a round under what it is about.
        let (between, under) = match card.body {
            CardBody::Round(_) => (0, unheld),
            CardBody::Choosing { .. } | CardBody::Saying { .. } => (unheld, 0),
        };
        let cursor = held
            .cursor
            .map(|(row, cell)| (row + title_blank + above.page + between, cell))
            .filter(|(row, _)| *row < room.rows);
        let blank = |rows: usize| std::iter::repeat_with(Line::new).take(rows);
        let mut lines: Vec<Line> = blank(title_blank).collect();
        lines.extend(above.lines.into_iter().skip(top).take(above.page));
        lines.extend(blank(between));
        lines.extend(held.lines);
        lines.extend(blank(under));
        lines.extend(below);
        lines.truncate(room.rows);
        Paned {
            lines,
            cursor,
            laid: Laid {
                top,
                page: above.page,
                most,
            },
        }
    }

    /// Whether `card`'s fewest rows ([`Card::least`]) stand in the pane `room` says, `blocks`
    /// and `abridged` laid out around it as [`Terminal::card_at_the_foot`] lays them out: what
    /// decides whether a region's title gives way to its card.
    pub(super) fn keeps_the_cards_least(
        &self,
        (blocks, abridged): (&[Block], &[Vec<Block>]),
        (at, card): (usize, &Card),
        room: Room,
        spacing: Spacing,
    ) -> bool {
        let footing = self.footing((blocks, abridged), (at, card), room, spacing);
        footing.card_rows >= card.least(&footing.on.look)
    }

    /// `blocks` around `card`, at `at` among them, and `abridged`, laid out in the pane `room`
    /// says, spaced as `spacing` says, by the rule this module states.
    fn footing(
        &self,
        (blocks, abridged): (&[Block], &[Vec<Block>]),
        (at, card): (usize, &Card),
        room: Room,
        spacing: Spacing,
    ) -> Footing {
        let on = self.measured_for(card);
        let below = on.under_a_card(&blocks[at + 1..], spacing);
        let left = room.rows.saturating_sub(below.len());
        let forms = on.above_a_card((&blocks[..at], abridged), spacing);
        let parting = |lines: &[Line]| match spacing {
            Spacing::Aired => parted(lines.to_vec(), Side::Above),
            Spacing::Close => lines.to_vec(),
        };
        let above = match room.shortest {
            true => on.over_its_least(card, &forms, parting, (left, room.under_a_title)),
            false => on.over_it_whole(card, &forms, parting, (left, room.under_a_title)),
        };
        let card_rows = left.saturating_sub(usize::from(above.title_blank) + above.page);
        Footing {
            on,
            below,
            above,
            card_rows,
        }
    }

    /// This terminal measured for `card` and what stands around it: a round spans at most
    /// [`ROUND_MOST`] cells, a decision's card the pane's width.
    fn measured_for(&self, card: &Card) -> Terminal {
        let width = match card.body {
            CardBody::Round(_) => self.look.width.within(ROUND_MOST),
            CardBody::Choosing { .. } | CardBody::Saying { .. } => self.look.width,
        };
        self.at_width(width)
    }

    /// The lines of `blocks`, what stands under a card, spaced as `spacing` says, a blank row
    /// parting them from the card where it airs them.
    fn under_a_card(&self, blocks: &[Block], spacing: Spacing) -> Vec<Line> {
        let below = closed_up(self.in_rhythm(blocks), spacing);
        match spacing {
            Spacing::Aired => parted(below, Side::Below),
            Spacing::Close => below,
        }
    }

    /// The lines of `whole`, what stands above a card, then of each of `abridged`, what it
    /// says in fewer rows, spaced as `spacing` says, with nothing parting them from the card.
    fn above_a_card(
        &self,
        (whole, abridged): (&[Block], &[Vec<Block>]),
        spacing: Spacing,
    ) -> Vec<Vec<Line>> {
        std::iter::once(whole)
            .chain(abridged.iter().map(Vec::as_slice))
            .map(|blocks| closed_up(self.in_rhythm(blocks), spacing))
            .collect()
    }

    /// What stands above `card` at the shortest look, in `left` rows, a blank row under the
    /// region's title where `under_a_title` owes one: the rows the card's least leaves, `forms`
    /// the whole of what stands above then each abridgement, `parting` setting each apart from
    /// the card; the first that fits, else the whole, from its top.
    fn over_its_least(
        &self,
        card: &Card,
        forms: &[Vec<Line>],
        parting: impl Fn(&[Line]) -> Vec<Line>,
        (left, under_a_title): (usize, bool),
    ) -> Above {
        let room = left
            .saturating_sub(usize::from(under_a_title))
            .saturating_sub(card.least(&self.look));
        let parted: Vec<Vec<Line>> = forms.iter().map(|form| parting(form)).collect();
        let lines = parted
            .iter()
            .find(|lines| lines.len() <= room)
            .unwrap_or(&parted[0])
            .clone();
        Above {
            title_blank: under_a_title,
            page: lines.len().min(room),
            lines,
        }
    }

    /// What stands above `card` taller than the shortest look, in `left` rows, a blank row
    /// under the region's title where `under_a_title` owes one, the card standing whole: of
    /// `forms`, the whole of what stands above then each abridgement, the whole set apart by
    /// `parting`, then each with nothing parting it, then each without the blank row under the
    /// title; the first that fits. Where none does, the whole gives way from its top in the
    /// rows the card leaves.
    fn over_it_whole(
        &self,
        card: &Card,
        forms: &[Vec<Line>],
        parting: impl Fn(&[Line]) -> Vec<Line>,
        (left, under_a_title): (usize, bool),
    ) -> Above {
        let whole = card.whole(&self.look);
        let set_apart = std::iter::once((under_a_title, parting(&forms[0])));
        let close = forms.iter().map(|form| (under_a_title, form.clone()));
        let untitled = forms
            .iter()
            .filter(|_| under_a_title)
            .map(|form| (false, form.clone()));
        let fits = set_apart
            .chain(close)
            .chain(untitled)
            .find(|(title_blank, lines)| usize::from(*title_blank) + lines.len() + whole <= left);
        if let Some((title_blank, lines)) = fits {
            return Above {
                title_blank,
                page: lines.len(),
                lines,
            };
        }
        let room = left
            .saturating_sub(usize::from(under_a_title))
            .saturating_sub(whole);
        let lines = parting(&forms[0]);
        Above {
            title_blank: under_a_title,
            page: lines.len().min(room),
            lines,
        }
    }
}

#[cfg(test)]
mod tests;
