//! See [`super`]. How a region is laid out in a pane, classified once from its document and
//! from what the region says it is: a card at its foot, blocks over the field in perspective, a
//! list of keys alone, blocks over a feed at its foot, a form alone, another prompt among
//! blocks, a list of runs, or a page. Each kind carries what its drawing reads, what the region
//! says in fewer rows where that kind reads it; how the pane spaces it, whether its title gives
//! way and how it is drawn all match on the one kind.
//!
//! The kind is read in order: a card wherever it stands among the blocks; then what the
//! document ends with, a horizon, a list of keys that is its one block, or a feed; then what
//! it holds, a prompt, a form alone among prompts; then a list of runs where the region says it
//! lists runs, as the field's does ([`Region::lists_runs`]), whatever it lists at the moment:
//! never by whether it holds a heading over runs, which a field with no run yet does not; else
//! a page. To add a kind of block, name it in [`ending`]'s match, which matches every kind,
//! saying whether a document that ends with it is laid out by it; a block that is a prompt is
//! classified by [`Block::prompt`]. To add a kind of region, add a variant here: every match
//! on it then asks how it is spaced and drawn.

use super::fed::Foot;
use super::listed::Listed;
use crate::components::{Card, Form, Horizon, KeyList, Region};
use crate::doc::{Block, Doc, Prompt};

/// How a region is laid out in a pane, classified once from its document ([`Laying::of`]).
/// Not the region's spacing, which this decides with the screen.
pub(super) enum Laying<'a> {
    /// A card at the pane's foot under what it rests on, the blocks around the card `blocks`
    /// and the card the block at `at`, what stands above it said in fewer rows as `shorter`
    /// says.
    Card {
        blocks: &'a [Block],
        at: usize,
        card: &'a Card,
        shorter: &'a [Vec<Block>],
    },
    /// The blocks `above` over the field in perspective, said in fewer rows as `shorter` says.
    Horizon {
        above: &'a [Block],
        horizon: &'a Horizon,
        shorter: &'a [Vec<Block>],
    },
    /// A list of keys, the document's one block, as help is.
    Keys(&'a KeyList),
    /// `blocks` over the feed at the document's foot, `foot`, what stands above it said in
    /// fewer rows as `shorter` says.
    Fed {
        blocks: &'a [Block],
        foot: Foot<'a>,
        shorter: &'a [Vec<Block>],
    },
    /// A form, the document's one block, as a new run's rows: held as a prompt is, and holding
    /// at its foot what a region beside it that gave way acts on.
    Form { blocks: &'a [Block], form: &'a Form },
    /// A prompt other than a card, the block at `at`, among `blocks`.
    Prompted { blocks: &'a [Block], at: usize },
    /// A list of runs, a pointer on one of them, as the field lists them: under their headings,
    /// below the rows that stand over them, none yet in a field with no run.
    Listed(Listed<'a>),
    /// A page read from its top or along a pointer, `blocks` said in fewer rows as `shorter`
    /// says.
    Page {
        blocks: &'a [Block],
        shorter: &'a [Vec<Block>],
    },
}

impl<'a> Laying<'a> {
    /// How `region` is laid out.
    pub(super) fn of(region: &'a Region) -> Laying<'a> {
        Laying::of_a_doc(&region.doc, &region.abridged, region.lists_runs)
    }

    /// How a document `doc`, said in fewer rows as `shorter` says, is laid out by the order
    /// this module states, in a region that `lists_runs` where it says so.
    pub(super) fn of_a_doc(
        doc: &'a Doc,
        shorter: &'a [Vec<Block>],
        lists_runs: bool,
    ) -> Laying<'a> {
        let blocks = doc.blocks();
        let prompt = doc
            .prompt_at()
            .and_then(|at| blocks[at].prompt().map(|prompt| (at, prompt)));
        match prompt {
            Some((at, Prompt::Card(card))) => {
                return Laying::Card {
                    blocks,
                    at,
                    card,
                    shorter,
                }
            }
            Some((
                _,
                Prompt::Desk(_)
                | Prompt::TypedAnswer(_)
                | Prompt::ChoiceList(_)
                | Prompt::Form(_)
                | Prompt::Survey(_),
            ))
            | None => {}
        }
        let ended = match blocks.last().map(ending) {
            Some(Ending::Horizon(horizon)) => Some(Laying::Horizon {
                above: &blocks[..blocks.len() - 1],
                horizon,
                shorter,
            }),
            Some(Ending::Keys(list)) if blocks.len() == 1 => Some(Laying::Keys(list)),
            Some(Ending::Feed) => Foot::of(doc).map(|foot| Laying::Fed {
                blocks,
                foot,
                shorter,
            }),
            Some(Ending::Keys(_) | Ending::Held) | None => None,
        };
        ended.unwrap_or_else(|| Laying::by_what_it_holds(doc, shorter, lists_runs))
    }

    /// How a document `doc` that neither holds a card nor ends with what lays it out is laid
    /// out by what it holds: a form alone, another prompt among blocks, a list of runs where
    /// its region `lists_runs`, else a page said in fewer rows as `shorter` says.
    fn by_what_it_holds(doc: &'a Doc, shorter: &'a [Vec<Block>], lists_runs: bool) -> Laying<'a> {
        let blocks = doc.blocks();
        match (doc.prompt_at(), blocks) {
            (Some(_), [Block::Form(form)]) => Laying::Form { blocks, form },
            (Some(at), _) => Laying::Prompted { blocks, at },
            (None, _) if lists_runs => Laying::Listed(Listed::of(doc)),
            (None, _) => Laying::Page { blocks, shorter },
        }
    }
}

/// What a document that ends with a block is laid out by, where that block decides it.
enum Ending<'a> {
    /// The field in perspective, under every block above it.
    Horizon(&'a Horizon),
    /// A list of keys, which lays the region out as its keys where it is the one block.
    Keys(&'a KeyList),
    /// A feed, or a section that may hold one alone, at the document's foot.
    Feed,
    /// None: the document is laid out by what it holds.
    Held,
}

/// What a document that ends with `last` is laid out by. Every kind of block is named here,
/// so a new kind stops the build until it says whether it lays its region out.
fn ending(last: &Block) -> Ending<'_> {
    match last {
        Block::Horizon(horizon) => Ending::Horizon(horizon),
        Block::KeyList(list) => Ending::Keys(list),
        Block::Feed(_) | Block::Section(_) => Ending::Feed,
        Block::Headline(_)
        | Block::Furrow(_)
        | Block::Ribbon(_)
        | Block::Gist(_)
        | Block::Fields(_)
        | Block::Labelled(_)
        | Block::Table(_)
        | Block::Nodes(_)
        | Block::Runs(_)
        | Block::AtAGlance(_)
        | Block::Evidence(_)
        | Block::Said(_)
        | Block::Paths(_)
        | Block::Diffstat(_)
        | Block::Decision(_)
        | Block::Card(_)
        | Block::Desk(_)
        | Block::TypedAnswer(_)
        | Block::ChoiceList(_)
        | Block::WordChoice(_)
        | Block::Form(_)
        | Block::Survey(_)
        | Block::Checklist(_)
        | Block::Findings(_)
        | Block::Problem(_)
        | Block::Refusal(_)
        | Block::Next(_)
        | Block::SameAtAShell(_)
        | Block::Legend(_)
        | Block::Attention(_)
        | Block::Noted(_)
        | Block::Title(_)
        | Block::Heading(_)
        | Block::Fold(_)
        | Block::Prose(_)
        | Block::Note(_)
        | Block::Markdown(_)
        | Block::Marked(_)
        | Block::Concern(_)
        | Block::Diagram(_)
        | Block::Code(_)
        | Block::Lines(_)
        | Block::Rows(_)
        | Block::Meter(_)
        | Block::Wordmark(_) => Ending::Held,
    }
}

/// Whether the prompt `blocks` hold, the first among them, is a plan's desk, which lays out
/// columns of its own across the pane and stands close on a screen standing short of rows
/// wherever it stands.
pub(super) fn holds_a_desk(blocks: &[Block]) -> bool {
    matches!(blocks.iter().find_map(Block::prompt), Some(Prompt::Desk(_)))
}

#[cfg(test)]
mod tests;
