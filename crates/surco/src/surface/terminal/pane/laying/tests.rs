//! Each kind of region the catalog's blocks build is classified once, in the order the module
//! states: a card wherever it stands, then what the document ends with, then what it holds.

use super::Laying;
use crate::components::{Horizon, Prose, Region, Sight};
use crate::doc::{Block, Doc};
use crate::line::Tone;

/// What `laying` is called, every kind named.
fn kind(laying: &Laying<'_>) -> &'static str {
    match laying {
        Laying::Card { .. } => "card",
        Laying::Horizon { .. } => "horizon",
        Laying::Keys(_) => "keys",
        Laying::Fed { .. } => "fed",
        Laying::Form { .. } => "form",
        Laying::Prompted { .. } => "prompted",
        Laying::Listed(_) => "listed",
        Laying::Page { .. } => "page",
    }
}

/// The catalog's first block that `is` the kind asked for.
fn catalogued(is: fn(&Block) -> bool) -> Block {
    let blocks = crate::components::catalog::catalog().into_blocks();
    blocks.into_iter().find(is).expect("the catalog holds one")
}

/// `blocks`' region, read along a pointer, classified.
fn classified(blocks: Vec<Block>) -> &'static str {
    let region = Region::new(Doc::new().with_all(blocks)).in_sight(Sight::following_pointer());
    kind(&Laying::of(&region))
}

/// Where a test sets the block whose region it classifies.
#[derive(Clone, Copy)]
enum Stands {
    Alone,
    OverTheField,
    UnderASentence,
}

/// `block` set where `stands` says.
fn set(block: Block, stands: Stands) -> Vec<Block> {
    match stands {
        Stands::Alone => vec![block],
        Stands::OverTheField => vec![block, Horizon { tone: Tone::Done }.into()],
        Stands::UnderASentence => vec![Prose("about it".to_string()).into(), block],
    }
}

/// The catalog's block that `is` the kind asked for, set where each case stands it, classified
/// as the case says.
fn hold(is: fn(&Block) -> bool, cases: &[(Stands, &str)]) {
    for (stands, kind) in cases {
        assert_eq!(classified(set(catalogued(is), *stands)), *kind);
    }
}

/// A card is the region's kind wherever it stands; then what the document ends with: the
/// field in perspective, a list of keys that is its one block, a feed.
#[test]
fn a_card_and_then_what_a_document_ends_with_classify_it_first() {
    use Stands::{Alone, OverTheField, UnderASentence};
    hold(
        |block| matches!(block, Block::Card(_)),
        &[(Alone, "card"), (OverTheField, "card")],
    );
    hold(
        |block| matches!(block, Block::TypedAnswer(_)),
        &[(OverTheField, "horizon"), (UnderASentence, "prompted")],
    );
    hold(
        |block| matches!(block, Block::KeyList(_)),
        &[(Alone, "keys"), (UnderASentence, "page")],
    );
    hold(
        |block| matches!(block, Block::Feed(_)),
        &[(UnderASentence, "fed")],
    );
}

/// Then what the document holds: a form alone, another prompt among blocks, a list of runs
/// where the region says it lists runs, whatever it holds then, else a page.
#[test]
fn what_a_document_holds_classifies_the_rest() {
    use Stands::{Alone, UnderASentence};
    hold(
        |block| matches!(block, Block::Form(_)),
        &[(Alone, "form"), (UnderASentence, "prompted")],
    );
    let prompts: [fn(&Block) -> bool; 3] = [
        |block| matches!(block, Block::Desk(_)),
        |block| matches!(block, Block::ChoiceList(_)),
        |block| matches!(block, Block::Survey(_)),
    ];
    for is in prompts {
        hold(is, &[(UnderASentence, "prompted")]);
    }
    hold(
        |block| matches!(block, Block::Checklist(_)),
        &[(UnderASentence, "page")],
    );
    hold(|block| matches!(block, Block::Runs(_)), &[(Alone, "page")]);
    let headed = || {
        vec![
            Block::Heading("needs you".to_string()),
            catalogued(|block| matches!(block, Block::Runs(_))),
        ]
    };
    assert_eq!(
        classified(headed()),
        "page",
        "runs under a heading, in a region that says it lists none"
    );
    let listing = |blocks: Vec<Block>| {
        let region = Region::new(Doc::new().with_all(blocks)).listing_runs();
        kind(&Laying::of(&region))
    };
    assert_eq!(listing(headed()), "listed");
    let no_run_yet = vec![Prose("start one".to_string()).into()];
    assert_eq!(listing(no_run_yet), "listed", "a field with no run yet");
}
