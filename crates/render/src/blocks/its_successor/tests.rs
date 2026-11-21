//! The run a promotion handed a run's work to, under its heading: that none came, where the
//! follower waited the time one takes to be created and none did.

use surco::doc::Doc;
use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::surface::{Surface, Terminal};

use super::ItsSuccessor;

#[test]
fn a_promotion_whose_successor_never_came_says_none_came_under_its_heading() {
    let blocks = ItsSuccessor::NoneCame.blocks(Glyphs::Unicode);
    let drawn = Terminal::on(Look::plain()).draw(&Doc::new().with_all(blocks));
    assert_eq!(
        drawn.lines().collect::<Vec<_>>(),
        [
            "its successor",
            "none came in the time one takes to be created"
        ]
    );
}
