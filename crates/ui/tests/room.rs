//! What a surface shows where it has less room than it lays a screen out in.

use surco::surface::{Markdown, Surface};

/// The document says the room needed and the room there is.
#[test]
fn the_room_needed_says_both_sizes() {
    let said = Markdown.draw(&yunta_ui::room_needed((60, 14), (59, 14)));
    assert!(said.contains("60 by 14"), "{said}");
    assert!(said.contains("59 by 14"), "{said}");
}
