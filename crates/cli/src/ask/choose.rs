//! The one list a person picks from, held at a terminal until it is answered.

use yunta_ui::prompt::{Alternative, Picker};

use super::held::hold;
use super::{Answered, Console};

/// Puts `alternatives` to the person and returns the value behind the one picked. What
/// they answer is said above it, by the caller.
pub(crate) fn choose<T: Clone>(
    console: &Console,
    alternatives: Vec<Alternative<T>>,
) -> Answered<T> {
    hold(console, &mut Picker::new(alternatives, console.escape()))
}
