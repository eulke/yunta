//! A region of a screen drawn in a pane: the one rule an engine of cells draws a region by,
//! so every surface and every test of one draws a region the same way. A region is
//! classified once from its document (`laying`, which says how to add a kind), and how the
//! pane spaces it, whether its title gives way and how it is drawn all match on that kind.
//! A region whose document holds a prompt holds it, fitted to the rows the blocks around it
//! leave, the cursor where it is typed into; any other scrolls from what of it the person
//! asked to see. A pane is no terminal's bottom row: a prompt in one fills every row it is
//! given, where one held where a person is ([`Terminal::held`]) keeps the last.
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
//! A region that lists runs, as the field does, is laid out as its lists whatever it lists at
//! the moment, none among them, a cell inside its edges: short of rows, the blank rows between
//! two lists give way first, then a window that keeps the run pointed at in sight counts the
//! runs it hides; standing close, what stands above the lists gives way before the window
//! opens (`listed`). Where a region beside the main one gave way, the main region holds what a
//! person acts on from it ([`Terminal::pane_holding`]): a region that lists runs under its
//! lists, which give way first; a form alone at its foot, a part of its own it gives way
//! with; any other under what it shows.
//!
//! A region whose document ends with a horizon, the field in perspective, lays its blocks out
//! first, a line asked among them held as any prompt is, then the horizon in every row they
//! leave, a blank row parting them (`horizon`). Given fewer rows than the horizon's least, it
//! draws none; short of rows, the blocks' blank rows give way before their rows scroll, but a
//! survey's (a setup's rows), which holds what stands above it as its drawing says. Words set
//! to a measure narrower than the pane stand in a column that wide, centred (`measured`).
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
//! a blank row between two of its blocks. Short of rows, as at the least terminal, where a
//! node's pane holds five rows, it stands close ([`Terminal::pane_short_of_rows`]): no blank
//! row under its title nor between its blocks, each row one of what it says. Above a feed,
//! what gives way is counted with the blank rows standing, which then go, their rows the
//! window's. A prompt held in a pane keeps its own blank rows, which its drawing lets go in
//! its own order, but a line or a list asked on a page of its own (`asked`). Short of rows,
//! a page stands close too: an untitled region read from its top or along a pointer, holding
//! no prompt, feed or horizon, lets its blank rows go before its rows are counted under its
//! window, where a taller screen keeps them; and a plan's desk stands close. Whether a screen
//! stands short of rows is its layout's, the one signal every pane on it stands close by.

use super::Terminal;
use crate::components::{Laid, Measure, Region, Sight};
use crate::doc::{Block, Doc, Prompt};
use crate::line::Line;

mod asked;
mod card_at_the_foot;
mod fed;
mod fitted;
mod horizon;
mod laying;
mod listed;
mod measured;

use asked::asked_on_a_page;
use card_at_the_foot::Room;
use laying::{holds_a_desk, Laying};

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
    /// from its sight. A titled region opens a row under its title, a blank row parting them.
    pub fn pane(&self, region: &Region, rows: usize) -> Paned {
        self.region_paned(region, rows, false)
    }

    /// `region` drawn in a pane `rows` high as [`Terminal::pane`] draws it, on a screen
    /// standing short of rows, as the least terminal's does: a titled pane stands close there,
    /// with no blank row under its title nor between its blocks, and a page stands close too,
    /// before what it hides is counted under its window.
    pub fn pane_short_of_rows(&self, region: &Region, rows: usize) -> Paned {
        self.region_paned(region, rows, true)
    }

    /// `region` drawn in a pane `rows` high as [`Terminal::pane`] draws it, on a screen
    /// standing `short_of_rows` where its layout says so, holding `instead` under its own
    /// blocks: what a person acts on from a region beside it that gave way. A region that
    /// lists runs holds it under its lists, which give way first; an untitled region holding
    /// a form alone holds it at the form's foot, a part of the form's own that the form gives
    /// way with, a row of its window where its rows do not all fit; any other holds it under
    /// what it shows in the rows `instead` leaves it, a blank row parting them where the pane
    /// is aired.
    pub fn pane_holding(
        &self,
        region: &Region,
        instead: &[Block],
        rows: usize,
        short_of_rows: bool,
    ) -> Paned {
        if instead.is_empty() {
            return self.region_paned(region, rows, short_of_rows);
        }
        let spacing = match short_of_rows {
            true => Spacing::Close,
            false => Spacing::Aired,
        };
        match (region.title.is_empty(), Laying::of(region)) {
            (true, Laying::Form { form, .. }) => {
                let stead = closed_up(self.in_rhythm(instead), spacing);
                let held = self.form_held_with(form, stead, rows);
                return Paned {
                    lines: held.lines,
                    cursor: held.cursor,
                    laid: held.laid.unwrap_or_default(),
                };
            }
            (true, Laying::Listed(listed)) => {
                let (lines, laid) = self.listed(&listed.holding(instead), rows, spacing);
                return Paned {
                    lines,
                    cursor: None,
                    laid,
                };
            }
            (true | false, _) => {}
        }
        let held = closed_up(self.in_rhythm(instead), spacing);
        let parted = usize::from(spacing == Spacing::Aired);
        let left = rows.saturating_sub(held.len() + parted);
        let mut paned = self.region_paned(region, left, short_of_rows);
        if spacing == Spacing::Aired {
            paned.lines.push(Line::new());
        }
        paned.lines.extend(held);
        paned.lines.truncate(rows);
        paned
    }

    /// Whether `region`, drawn under its title in a pane `rows` high on a screen standing
    /// `short_of_rows` where its layout says so, keeps the title: it does, but where it holds a
    /// card whose fewest rows ([`Card::least`]) do not stand in the pane as it is drawn there,
    /// with what stands around the card. The title is what stands above the card there, and
    /// gives way to it as the rest of what stands above it does, before the card gives way in
    /// its own order: the region is drawn in its title's row too
    /// ([`Terminal::pane_in_place_of_its_title`]).
    ///
    /// [`Card::least`]: crate::components::Card
    pub fn keeps_its_title(&self, region: &Region, rows: usize, short_of_rows: bool) -> bool {
        let laying = Laying::of(region);
        let Laying::Card {
            blocks,
            at,
            card,
            shorter,
        } = laying
        else {
            return true;
        };
        if region.title.is_empty() {
            return true;
        }
        let spacing = laying.spacing(short_of_rows, true);
        let room = Room::under_the_title(rows, true, spacing, short_of_rows);
        self.keeps_the_cards_least((blocks, shorter), (at, card), room, spacing)
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
        let laying = Laying::of(region);
        let spacing = laying.spacing(short_of_rows, titled);
        let parted = titled && spacing == Spacing::Aired;
        if let Laying::Card {
            blocks,
            at,
            card,
            shorter,
        } = laying
        {
            let room = Room::under_the_title(rows, titled, spacing, short_of_rows);
            return self.card_at_the_foot(
                (blocks, shorter),
                (at, card),
                region.sight,
                room,
                spacing,
            );
        }
        let (sight, measure) = (region.sight, region.measure);
        if !parted || rows == 0 {
            return self.spaced(&laying, sight, rows, spacing, measure);
        }
        let mut paned = self.spaced(&laying, sight, rows - 1, spacing, measure);
        paned.lines.insert(0, Line::new());
        paned.cursor = paned.cursor.map(|(row, cell)| (row + 1, cell));
        paned
    }

    /// `doc`, of which `sight` is what a person asked to see, drawn in a pane `rows` high
    /// by the rule [`Terminal::pane`] states, its blank rows standing: what is open over a
    /// screen is drawn by.
    pub(super) fn paned(&self, doc: &Doc, sight: Sight, rows: usize) -> Paned {
        let laying = Laying::of_a_doc(doc, &[], false);
        self.spaced(&laying, sight, rows, Spacing::Aired, None)
    }

    /// A region laid out as `laying` says, of which `sight` is what a person asked to see,
    /// drawn in a pane `rows` high by the rule [`Terminal::pane`] states, spaced as `spacing`
    /// says, its words set to `measure` where it gives one: in a column that wide, centred
    /// where the pane is wider; the field in perspective spans the pane under them.
    fn spaced(
        &self,
        laying: &Laying<'_>,
        sight: Sight,
        rows: usize,
        spacing: Spacing,
        measure: Option<Measure>,
    ) -> Paned {
        if let Laying::Horizon {
            above,
            horizon,
            shorter,
        } = laying
        {
            let over = (*above, *horizon);
            return self.over_a_horizon(over, shorter, sight, rows, spacing, measure);
        }
        match self.column(measure) {
            Some(column) => {
                let paned = column.words.laid_across(laying, sight, rows, spacing);
                column.placed(paned)
            }
            None => self.laid_across(laying, sight, rows, spacing),
        }
    }

    /// A region laid out as `laying` says, of which `sight` is what a person asked to see,
    /// drawn across a pane `rows` high by the rule [`Terminal::pane`] states, spaced as
    /// `spacing` says, its words as wide as the pane.
    fn laid_across(
        &self,
        laying: &Laying<'_>,
        sight: Sight,
        rows: usize,
        spacing: Spacing,
    ) -> Paned {
        let scrolled = |(lines, laid): (Vec<Line>, Laid)| Paned {
            lines,
            cursor: None,
            laid,
        };
        match laying {
            // A card outside a region, as what is open over a screen holds one, keeps its least
            // as a pane at the shortest look does.
            Laying::Card {
                blocks,
                at,
                card,
                shorter,
            } => {
                let room = Room {
                    rows,
                    under_a_title: false,
                    shortest: true,
                };
                self.card_at_the_foot((*blocks, *shorter), (*at, *card), sight, room, spacing)
            }
            Laying::Horizon {
                above,
                horizon,
                shorter,
            } => self.over_a_horizon((*above, *horizon), shorter, sight, rows, spacing, None),
            // A list of keys alone, as help is, gives way as a list does rather than scroll as
            // lines: its blank rows first, then its window counts the keys it hides.
            Laying::Keys(list) => scrolled(list.held(&self.look, sight, rows)),
            Laying::Fed { foot, shorter, .. } => self.fed(foot, shorter, sight, rows, spacing),
            Laying::Form { blocks, .. } => self.prompted(blocks, 0, rows, spacing),
            Laying::Prompted { blocks, at } => self.prompted(blocks, *at, rows, spacing),
            Laying::Listed(listed) => scrolled(self.listed(listed, rows, spacing)),
            Laying::Page { blocks, shorter } => {
                let lines = self.fitted(blocks, shorter, rows, spacing);
                scrolled(self.scrolled_lines(&lines, sight, rows))
            }
        }
    }

    /// `blocks`, whose block at `at` is a prompt other than a card, drawn in a pane `rows`
    /// high by the rule [`Terminal::pane`] states: what stands under the prompt whole, what
    /// stands above it in at most half of what that leaves, and the prompt in the rest; a
    /// plan's desk in a pane that stands close stands close itself.
    fn prompted(&self, blocks: &[Block], at: usize, rows: usize, spacing: Spacing) -> Paned {
        if let ([alone @ Block::TypedAnswer(_)], Spacing::Aired) = (blocks, spacing) {
            return self.line_asked_alone(alone, rows);
        }
        let below = parted(self.in_rhythm(&blocks[at + 1..]), Side::Below);
        if let Some(Prompt::Survey(survey)) = blocks[at].prompt() {
            let above = parted(self.in_rhythm(&blocks[..at]), Side::Above);
            return self.survey_held(above, survey, below, rows);
        }
        let left = rows.saturating_sub(below.len());
        let above = self.in_rhythm(&blocks[..at]);
        let mut above = match spacing {
            Spacing::Close if asked_on_a_page(blocks) => closed_up(above, spacing),
            Spacing::Close | Spacing::Aired => parted(above, Side::Above),
        };
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

impl Laying<'_> {
    /// How a pane spaces a region laid out so, under its title where it stands `titled`, on a
    /// screen standing `short_of_rows` where its layout says so: there a titled pane stands
    /// close, and so does a page that says itself in no fewer rows, runs listed along a
    /// pointer, a plan's desk, and a line or a list asked on a page of its own; any other pane
    /// is aired.
    fn spacing(&self, short_of_rows: bool, titled: bool) -> Spacing {
        let close_untitled = match self {
            Laying::Page { shorter, .. } => shorter.is_empty(),
            Laying::Listed(_) => true,
            Laying::Prompted { blocks, .. } => holds_a_desk(blocks) || asked_on_a_page(blocks),
            Laying::Horizon { above: blocks, .. } | Laying::Fed { blocks, .. } => {
                holds_a_desk(blocks)
            }
            Laying::Card { .. } | Laying::Keys(_) | Laying::Form { .. } => false,
        };
        match short_of_rows && (titled || close_untitled) {
            true => Spacing::Close,
            false => Spacing::Aired,
        }
    }
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
