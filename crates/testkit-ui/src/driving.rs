//! A state driven with its performer as a surface drives it, with no terminal and no
//! window: each request carried out, each answer handed back, and the strokes a test
//! presses once the state waits on nothing.

use std::sync::Arc;

use tokio::sync::mpsc;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, State};

/// `state` driven with `performer`: what `asked` asks for and every request it leads to
/// carried out, each answer handed back, then each of `strokes` in turn once the state
/// waits on nothing; the state as it stands when it waits on nothing and no stroke is
/// left, with what only a surface does, showing a page or leaving, that it asked for.
pub async fn drive(
    mut state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> (State, Vec<Request>) {
    let (inputs, mut received) = mpsc::unbounded_channel();
    let mut strokes = strokes.iter().copied();
    let (mut asked, mut surfaced) = (asked, Vec::new());
    loop {
        for request in asked.drain(..) {
            surfaced.extend(performer.take(request, &inputs));
        }
        if state.waiting_on().is_empty() {
            match strokes.next() {
                Some(stroke) => asked = state.update(Input::Stroke(stroke)),
                None => return (state, surfaced),
            }
            continue;
        }
        let input = received
            .recv()
            .await
            .expect("the performer hands back what the state waits on");
        asked = state.update(input);
    }
}
