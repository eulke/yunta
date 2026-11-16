//! A session held by a word. The step that awaits one holds the session on
//! the filesystem, the one thing a test and a session in another process
//! both reach: the player looks for the word's file at a pace until it
//! stands, its directory is gone, or a stop arrives. It says nothing on the
//! session's stream while it waits.

use std::path::Path;
use std::time::Duration;

use yunta_core::pace::Pace;

use super::script::Stops;

/// How often a held session looks for its word: a word given is heard within
/// a hundredth of a second, and each look is one `stat` of a file, which a
/// waiting session can afford a hundred times a second.
const LOOK_FOR_THE_WORD: Pace = Pace::every(Duration::from_millis(10));

/// Waits for the file `word` to stand. `false` once the directory holding it
/// is gone, since nobody is left to give it, or once one of `stops` arrives
/// first: the session ends there, as a stop ends it.
pub(super) async fn heard(word: &Path, stops: &Stops) -> bool {
    let marks = word.parent().unwrap_or(word);
    loop {
        if word.exists() {
            return true;
        }
        if !marks.exists() {
            return false;
        }
        tokio::select! {
            () = LOOK_FOR_THE_WORD.beat() => {}
            () = stops.awaited() => return false,
        }
    }
}
