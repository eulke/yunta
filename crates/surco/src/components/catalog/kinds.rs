//! Every kind of block stands in the catalog, whose goldens say how each is drawn on
//! every surface: a block without a sample is one nothing holds to.

use std::collections::BTreeSet;

use super::catalog;
use crate::doc::Block;

/// Names each kind of block once, in one list that both [`kind_of`] and [`KINDS`] are made
/// from: [`kind_of`] matches every variant, so a variant added to [`Block`] stops the build
/// until it is named here, and naming it puts it among the kinds the catalog is asked for.
macro_rules! kinds {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// What a block is called.
        fn kind_of(block: &Block) -> &'static str {
            match block {
                $(Block::$variant(_) => $name,)*
            }
        }

        /// Every kind there is, as [`kind_of`] names it.
        const KINDS: &[&str] = &[$($name),*];
    };
}

kinds!(
    Headline => "headline",
    Furrow => "furrow",
    Ribbon => "ribbon",
    Gist => "gist",
    Fields => "fields",
    Labelled => "labelled",
    Table => "table",
    Nodes => "nodes",
    Runs => "runs",
    AtAGlance => "at a glance",
    Evidence => "evidence",
    Said => "said",
    Paths => "paths",
    Diffstat => "diffstat",
    Decision => "decision",
    Card => "card",
    Desk => "desk",
    TypedAnswer => "typed answer",
    ChoiceList => "choice list",
    WordChoice => "word choice",
    Form => "form",
    Survey => "survey",
    Checklist => "checklist",
    Findings => "findings",
    Problem => "problem",
    Refusal => "refusal",
    Next => "next",
    SameAtAShell => "same at a shell",
    Legend => "legend",
    KeyList => "key list",
    Attention => "attention",
    Noted => "noted",
    Title => "title",
    Heading => "heading",
    Section => "section",
    Fold => "fold",
    Prose => "prose",
    Note => "note",
    Markdown => "markdown",
    Marked => "marked",
    Concern => "concern",
    Diagram => "diagram",
    Code => "code",
    Lines => "lines",
    Rows => "rows",
    Feed => "feed",
    Meter => "meter",
    Wordmark => "wordmark",
    Horizon => "horizon",
);

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
        .iter()
        .copied()
        .filter(|kind| !seen.contains(kind))
        .collect();
    assert!(missing.is_empty(), "no sample of: {}", missing.join(", "));
}
