//! What a screen says that is no stage's: here, how much room the application needs on a
//! terminal too small for it.

use crate::screen::room_needed;

/// A terminal a column short is told the room it needs and the room it has, and how to
/// leave.
#[test]
fn a_terminal_too_small_is_told_the_room_it_needs_and_has() {
    assert_eq!(
        room_needed((60, 14), (59, 14)),
        "the application needs a terminal of 60 by 14 at least, and this one is 59 by 14: \
         make it larger, or press q to leave"
    );
}
