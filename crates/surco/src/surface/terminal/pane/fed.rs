//! See [`super`]. A region laid out as the feed its document ends with: only the feed
//! scrolls, under the blocks that stand still above it.
//!
//! The feed is the document's last block, under the heading right above it where one stands
//! there; or it stands one step under what is above it, the one block of a section that
//! ends the document, hanging from the section's title, as a board's feed stands under its
//! tasks. The heading a feed hangs from stays with it; a feed with none stands apart from
//! what is above it, a blank row between.
//!
//! The blocks above stand still and take their rows first; the feed's window takes the
//! rows they leave and fills down from under them. Where the window would keep fewer than
//! its least ([`FEED_LEAST`]), what stands above says itself in fewer rows: the first of
//! the region's abridgements that leaves the window its least, each its caller's order of
//! what gives way before the feed. Where none does, the last stands: a pane that stands
//! close, at the least terminal, keeps it whole, as what the region says at the least, and
//! gives the feed what is left, its newest row at least; an aired pane, or a region that
//! offers no abridgement, lets it give way from its top, a blank row left at the pane's top
//! with it, since a pane opens on what it says.
//!
//! At its end the window shows the feed's newest rows, the newest the pane's last row,
//! which never gives way; scrolled back, the rows from the place asked for, the rows above
//! and under them counted. The place and what was laid out are counted from the whole
//! feed's first row, the rows its caller left out above the ones it holds among them, so a
//! place stays on the rows in sight whatever grows above or under them; a page of it is the
//! rows in sight at its end, so a key that pages moves by the rows between its two counts
//! and a person who pages back reads every row.

use super::{closed_up, Paned, Spacing, Terminal};
use crate::components::{Feed, Section, Sight};
use crate::doc::{Block, Doc};
use crate::line::Line;
use crate::width::INDENT;

/// The fewest rows a feed's window keeps before the blocks above it give way, where the feed
/// holds more rows than that: the row that counts the rows above its window, and its two
/// newest rows, so what the node did last reads beside what it does now.
const FEED_LEAST: usize = 3;

/// The feed a region's document ends with, the heading it hangs from, and the blocks that
/// stand above them. Not [`Feed`], the rows it holds.
pub(super) struct Foot<'d> {
    /// The blocks above the feed and its heading, which give way before them.
    above: &'d [Block],
    /// The heading the feed hangs from, where one does.
    heading: Option<Heading<'d>>,
    feed: &'d Feed,
}

/// The heading a feed hangs from.
enum Heading<'d> {
    /// A heading right above the feed, at the column of what stands above it.
    Over(&'d Block),
    /// The title of the section the feed is the one block of: the feed stands one step under
    /// what is above it, under the section's title.
    Section(&'d Section),
}

impl<'d> Foot<'d> {
    /// The feed `doc` ends with, where it ends with one: its last block, under the heading
    /// right above it where one stands there; or the one block of the section it ends with.
    pub(super) fn of(doc: &'d Doc) -> Option<Foot<'d>> {
        match doc.blocks().split_last()? {
            (Block::Feed(feed), above) => match above.split_last() {
                Some((heading @ Block::Heading(_), above)) => Some(Foot {
                    above,
                    heading: Some(Heading::Over(heading)),
                    feed,
                }),
                _ => Some(Foot {
                    above,
                    heading: None,
                    feed,
                }),
            },
            (Block::Section(section), above) => match section.blocks.as_slice() {
                [Block::Feed(feed)] => Some(Foot {
                    above,
                    heading: Some(Heading::Section(section)),
                    feed,
                }),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether the feed stands one step under what is above it.
    fn under(&self) -> bool {
        matches!(self.heading, Some(Heading::Section(_)))
    }
}

impl Terminal {
    /// The feed `foot` holds at the foot of a pane `rows` high, its window from the row
    /// `sight` asks for, under what stands above it: the region's own blocks, or the first of
    /// `abridged` that leaves the window its least, else the last, kept whole where the pane
    /// stands close and given way from its top elsewhere; the window takes the rows they
    /// leave, and the rows their blank rows leave where the pane stands close.
    pub(super) fn fed(
        &self,
        foot: &Foot<'_>,
        abridged: &[Vec<Block>],
        sight: Sight,
        rows: usize,
        spacing: Spacing,
    ) -> Paned {
        let least = FEED_LEAST.min(foot.feed.whole());
        let mut said = std::iter::once(foot.above)
            .chain(abridged.iter().map(Vec::as_slice))
            .map(|above| self.over_the_feed(above, foot));
        let mut over = said.next().unwrap_or_default();
        while over.len() + least > rows {
            match said.next() {
                Some(fewer) => over = fewer,
                None => break,
            }
        }
        let keeps_whole = spacing == Spacing::Close && !abridged.is_empty();
        let mut lines = match keeps_whole {
            // At the least terminal what the region says at the least stands, and the feed
            // keeps what is left of its rows, its newest row at least.
            true => {
                let mut lines = closed_up(over, spacing);
                let kept = lines.len().min(rows.saturating_sub(1.min(least)));
                lines.drain(..lines.len() - kept);
                lines
            }
            false => {
                let kept = over.len().min(rows.saturating_sub(least));
                over.drain(..over.len() - kept);
                let opens = over
                    .iter()
                    .position(|line| !line.text().is_empty())
                    .unwrap_or(over.len());
                over.drain(..opens);
                closed_up(over, spacing)
            }
        };
        let left = rows - lines.len();
        let (window, laid) = match foot.under() {
            false => foot.feed.windowed(&self.look, sight, left),
            true => {
                let inner = self.one_step_under();
                let (window, laid) = foot.feed.windowed(&inner.look, sight, left);
                let window = window.into_iter().map(|line| line.under(INDENT)).collect();
                (window, laid)
            }
        };
        lines.extend(window);
        Paned {
            lines,
            cursor: None,
            laid,
        }
    }

    /// What stands over `foot`'s feed where `above` stands above it: `above` in the rhythm a
    /// page keeps, then the heading the feed hangs from, or a blank row parting the feed from
    /// what is above it where it hangs from none.
    fn over_the_feed(&self, above: &[Block], foot: &Foot<'_>) -> Vec<Line> {
        let mut lines = self.in_rhythm(above);
        match &foot.heading {
            Some(Heading::Over(heading)) => lines.extend(self.block(heading)),
            Some(Heading::Section(section)) => lines.extend(self.section_title(section)),
            None => {
                if lines.last().is_some_and(|line| !line.text().is_empty()) {
                    lines.push(Line::new());
                }
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests;
