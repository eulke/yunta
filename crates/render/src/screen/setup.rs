//! See [`super`]. Setting a repository up: where it stands on the trail, what stands
//! while it is found and written, and its keys.

use surco::components::Legend;

use crate::prompt::{Escape, ESC};

/// Where setting a repository up stands on the trail.
pub const SETUP: &str = "setup";

/// What stands while setting a repository up finds what it runs.
pub const FINDING_WHAT_IT_RUNS: &str =
    "finding what this repository runs, and the adapters this machine answers for";

/// What stands while a repository's setup is written.
pub const WRITING_THE_SETUP: &str = "writing the setup";

/// What a setup's writing came to when what was found for it was let go first: it is
/// found again, and asked again.
pub const SETUP_LET_GO: &str =
    "what setting this repository up found was let go before it was written";

/// The keys of a setup's questions, as `init -i` answers them: Enter answers, and an
/// empty line or Escape keeps what was detected.
pub fn setup_legend() -> Legend {
    Legend::new()
        .key("enter", "answers")
        .key(ESC, Escape::KeepsDefault.does())
}

/// The keys of a setup written: on to the first run, or leaving.
pub fn written_legend() -> Legend {
    Legend::new()
        .key("enter", "on to the first run")
        .key("q", "leaves yunta")
}
