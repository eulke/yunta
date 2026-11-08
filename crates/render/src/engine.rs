//! What holds a run whose log says it moves: the engine its registry names is
//! gone, or it is asking a person at its terminal.
//!
//! A pause is on the log, and [`crate::pause`] says it; these two are not. The
//! registry says them, which a caller reads as an
//! [`Engine`](yunta_replay::registry::Engine) and the frame measures as an
//! [`Asking`]. A question asked live is on no log until it is answered, so the
//! only place to answer it is where it is asked, and every sentence here says
//! where.
//!
//! An asking is said at three lengths, as `pause` says what a parked run
//! waits on: [`asking_on`] on a row's one line, [`asking_claim`] beside a
//! headline that already says how long and in a refusal, and
//! [`asking_in_full`] with how long.

use std::time::Duration;

use surco::figures::duration;
use yunta_replay::registry::Asking;

/// What holds a run whose log says it is moving and whose engine is gone.
pub const STALLED: &str = "no process is driving it: the engine that ran it is gone";

/// What holds a run whose engine is asking a person at its terminal: who
/// asks, at which terminal, and that the answer is given there.
///
/// The claim a page makes beside a headline that already says how long, and
/// a refusal makes: [`asking_in_full`] says how long as well.
pub fn asking_claim(asking: &Asking) -> String {
    asked(asking, None)
}

/// What holds a run whose engine is asking a person at its terminal, with
/// how long it has asked: [`asking_claim`] and the time beside it.
pub fn asking_in_full(asking: &Asking) -> String {
    asked(asking, Some(asking.waited))
}

/// Who is asking at its terminal, for `waited` when it is said, and where
/// the answer is given.
fn asked(asking: &Asking, waited: Option<Duration>) -> String {
    let who = asking
        .prompt
        .node
        .as_ref()
        .map_or_else(|| "the run".to_string(), |node| format!("node `{node}`"));
    let waited = waited.map(|waited| format!(" for {}", duration(waited)));
    format!(
        "{who} is asking at its terminal (pid {}){}; answer it there",
        asking.pid,
        waited.unwrap_or_default()
    )
}

/// Whom a live engine is asking, on the one line a row has room for: the
/// node, and that the answer is given at its terminal.
pub fn asking_on(asking: &Asking) -> String {
    match &asking.prompt.node {
        Some(node) => format!("node `{node}` asks at its terminal"),
        None => "the run asks at its terminal".to_string(),
    }
}

#[cfg(test)]
mod tests;
