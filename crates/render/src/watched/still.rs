//! See [`super`]. What a node's work standing still says above it, so a person reads why
//! nothing moves and what moves it again: when the run goes on alone, where it waits on a
//! limit's reset; the answer a person recorded, and that the run goes on once an engine
//! takes it up, where that answer awaits the resume that acts on it. Neither is tinted, and
//! no spinner turns beside either: the run does not move.

use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use yunta_core::events::PauseReason;
use yunta_replay::frame::RunFrame;
use yunta_replay::RunState;

/// What a run waiting on a limit's reset does then.
const GOES_ON_ALONE: &str = "it goes on alone at";

/// What a run whose answer awaits its resume does then.
const TAKEN_UP: &str = "the run goes on when an engine takes it up";

/// What the run framed as `frame`, its state `state`, says above its node's work while
/// that work stands still, in `glyphs`: when it goes on alone, where a limit's reset holds
/// it; the answer recorded and that an engine takes it up, where the answer awaits its
/// resume. None where nothing holds it still that it says.
pub fn still(frame: &RunFrame, state: &RunState, glyphs: Glyphs) -> Option<Line> {
    let reset = frame
        .phase
        .waiting_on()
        .and_then(|on| on.reason())
        .and_then(PauseReason::reset_wait);
    if let Some((_, at)) = reset {
        // The gist above says the instant whole, its day among it.
        let at = at.format(crate::TIME_OF_DAY_FORMAT);
        return Some(Line::new().plain(format!("{GOES_ON_ALONE} {at}")));
    }
    let answer = state.answer_awaiting_its_resume()?;
    let done = Mark::Done;
    Some(
        Line::new()
            .push(Tone::of(done), glyphs.mark(done).to_string())
            .plain(" ")
            .plain(crate::chronicle::resolution(answer))
            .push(Tone::Muted, format!(" {} {TAKEN_UP}", glyphs.sep())),
    )
}
