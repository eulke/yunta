//! One line asked at a terminal, held until it is answered, and what stays on the
//! screen after it: the question and the value that stands.

use surco::line::Line;
use yunta_render::prompt::{ANSWER, FOLDED};
use yunta_ui::prompt::LineAsked;

use super::held::hold;
use super::{Console, NoAnswer};

/// Asks for one line under `asked`, with `kept` standing where no answer is given: a line
/// left empty keeps it, and so does Escape or a terminal that cannot be drawn on.
///
/// What stays on the screen is the question and the value that stands, which is the
/// collapsed line rather than the keystrokes it came from. `None` where Ctrl-C stopped
/// what is running: nothing of the prompt stays, and no value stands.
pub(crate) fn ask_line(console: &Console, asked: &Line, kept: &str) -> Option<String> {
    let mut prompt = LineAsked::new(asked.clone());
    let typed = match hold(console, &mut prompt) {
        Ok(typed) => Some(typed),
        Err(NoAnswer::Interrupted) => return None,
        Err(_) => None,
    };
    let folded = typed.as_ref().is_some_and(|typed| typed.folded);
    let stands = typed
        .map(|typed| typed.value)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| kept.to_string());
    let said = [
        console.look().paint.line(asked),
        format!("{ANSWER}{stands}"),
    ];
    for line in said
        .iter()
        .map(String::as_str)
        .chain(folded.then_some(FOLDED))
    {
        // best-effort: the value stands whether or not it could be said.
        console.say(line).ok();
    }
    Some(stands)
}
