//! The row a working node gets for what one of its sessions said last.
//!
//! What a session says is its own prose, so the row draws it plain, after a
//! label that says whose it is. It is never evidence: the row moves no spin
//! and resets no quiet clock, which follow the log. Not the session's notes
//! on the log, which hold only a digest of the same words.

use surco::line::{Line, Tone};
use yunta_engine::said::Said;

/// What a session said last, as one row: whose it is, then its first
/// sentence. Every control character the sentence carries is a space by the
/// time a terminal reads it, as everything a session wrote is.
pub(super) fn row(said: &Said, detail: &str) -> Line {
    let whose = match &said.task_id {
        Some(task) => format!("{task} says: "),
        None => "says: ".to_string(),
    };
    Line::new()
        .plain(detail)
        .push(Tone::Muted, whose)
        .plain(surco::prose::first_sentence(&said.text))
}

#[cfg(test)]
mod tests;
