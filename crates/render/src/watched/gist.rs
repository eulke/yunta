//! The row that says what a watched run comes to, in the words of its gist.
//!
//! The words are [`state::gist`](crate::state::gist)'s, the sentence a run's page sets under
//! its headline, so the row a person watches and the page they open say the same; the tone
//! answers whether anything needs the person.

use surco::line::{Line, Tone};
use surco::words::Standing;
use yunta_replay::frame::RunFrame;

/// The row under a watched run's work that says what the run comes to: its gist, in the
/// person's tone while the run needs them and muted otherwise. `None` for a run whose word
/// says it all. Not [`state::gist`](crate::state::gist), the sentence alone.
pub fn gist(frame: &RunFrame) -> Option<Line> {
    let tone = match frame.standing {
        Standing::NeedsYou => Tone::NeedsYou,
        _ => Tone::Muted,
    };
    crate::state::gist(frame).map(|gist| Line::new().push(tone, gist))
}

#[cfg(test)]
mod tests;
