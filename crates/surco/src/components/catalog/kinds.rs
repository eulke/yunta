//! Every kind of block stands in the catalog, whose goldens say how each is drawn on
//! every surface: a block without a sample is one nothing holds to.

use std::collections::BTreeSet;

use super::catalog;
use crate::doc::Block;

/// What a block is called. Naming every kind here is what stops the build on a new one until
/// it is named, and then the test below asks the catalog for a sample of it.
fn kind_of(block: &Block) -> &'static str {
    match block {
        Block::Headline(_) => "headline",
        Block::Furrow(_) => "furrow",
        Block::Ribbon(_) => "ribbon",
        Block::Gist(_) => "gist",
        Block::Fields(_) => "fields",
        Block::Table(_) => "table",
        Block::Nodes(_) => "nodes",
        Block::Runs(_) => "runs",
        Block::Evidence(_) => "evidence",
        Block::Said(_) => "said",
        Block::Paths(_) => "paths",
        Block::Diffstat(_) => "diffstat",
        Block::Decision(_) => "decision",
        Block::Card(_) => "card",
        Block::Desk(_) => "desk",
        Block::TypedAnswer(_) => "typed answer",
        Block::ChoiceList(_) => "choice list",
        Block::Checklist(_) => "checklist",
        Block::Problem(_) => "problem",
        Block::Refusal(_) => "refusal",
        Block::Next(_) => "next",
        Block::Legend(_) => "legend",
        Block::Attention(_) => "attention",
        Block::Title(_) => "title",
        Block::Heading(_) => "heading",
        Block::Section(_) => "section",
        Block::Fold(_) => "fold",
        Block::Prose(_) => "prose",
        Block::Note(_) => "note",
        Block::Markdown(_) => "markdown",
        Block::Marked(_) => "marked",
        Block::Concern(_) => "concern",
        Block::Diagram(_) => "diagram",
        Block::Code(_) => "code",
        Block::Lines(_) => "lines",
    }
}

/// Every kind there is, as [`kind_of`] names it.
const KINDS: [&str; 35] = [
    "headline",
    "furrow",
    "ribbon",
    "gist",
    "fields",
    "table",
    "nodes",
    "runs",
    "evidence",
    "said",
    "paths",
    "diffstat",
    "decision",
    "card",
    "desk",
    "typed answer",
    "choice list",
    "checklist",
    "problem",
    "refusal",
    "next",
    "legend",
    "attention",
    "title",
    "heading",
    "section",
    "fold",
    "prose",
    "note",
    "markdown",
    "marked",
    "concern",
    "diagram",
    "code",
    "lines",
];

/// Adds to `seen` what `blocks` are, and what each of them holds.
fn seen_in(blocks: &[Block], seen: &mut BTreeSet<&'static str>) {
    for block in blocks {
        seen.insert(kind_of(block));
        match block {
            Block::Section(section) => seen_in(&section.blocks, seen),
            Block::Fold(fold) => seen_in(&fold.0.blocks, seen),
            _ => {}
        }
    }
}

#[test]
fn every_kind_of_block_stands_in_the_catalog() {
    let mut seen = BTreeSet::new();
    seen_in(catalog().blocks(), &mut seen);
    let missing: Vec<&str> = KINDS
        .into_iter()
        .filter(|kind| !seen.contains(kind))
        .collect();
    assert!(missing.is_empty(), "no sample of: {}", missing.join(", "));
}
