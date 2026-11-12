//! A terminal's end: a run whose own process ended while a process it started in the
//! terminal's session still holds the terminal open fails the test, rather than hang it.

use std::path::Path;

use super::Terminal;
use crate::wait::WAIT_DEADLINE;

#[test]
#[should_panic(expected = "the terminal never closed")]
fn a_terminal_a_process_the_run_started_holds_open_fails_rather_than_hangs() {
    let dir = tempfile::tempdir().unwrap();
    let held_past_the_deadline = format!("sleep {} & exit 0", WAIT_DEADLINE.as_secs() + 5);
    let mut terminal = Terminal::open_with(
        Path::new("/bin/sh"),
        dir.path(),
        dir.path(),
        &["-c", &held_past_the_deadline],
        &[],
    );
    terminal.ended();
}
