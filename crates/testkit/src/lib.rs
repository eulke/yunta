//! Shared test harness for the Yunta workspace.
//!
//! One canonical copy of every piece of scaffolding an integration test
//! needs — a git-repo fixture, a fixed clock, a binary runner, a pty a
//! run is driven on, a writer a test reads back, a run bench, the
//! frames a surface draws, gate doubles, a recording run observer — so a
//! change to the shape of a test run happens in one place, and so the
//! fixtures are hermetic by construction (git isolated from the
//! developer's global config, a fixed clock, an injected home and
//! terminal) rather than by each test remembering to be.
//!
//! It is a dev-dependency only: nothing here ships in a published crate.

mod bench;
mod bin;
mod capture;
mod clock;
mod frames;
mod interaction;
mod observer;
mod repo;
mod terminal;
mod wait;

pub use bench::{Bench, MOCK_CONFIG};
pub use bin::{run_id_from, run_yunta, stderr, stdout};
pub use capture::Captured;
pub use clock::{AtClock, FixedClock, FIXED_NOW};
pub use frames::{child_link, node_frame, run_frame};
pub use interaction::{ApproveEverything, ScriptedInteraction};
pub use observer::{Frame, RecordingObserver};
pub use repo::{git, git_output, init_repo, write, INITIAL_BRANCH};
pub use terminal::{runs_root, Terminal};
pub use wait::{wait_for, wait_for_async, wait_until, wait_until_async, WAIT_DEADLINE};

/// Runs the `yunta` binary from an integration test: `yunta_in!(dir, home,
/// &["run", "wf.yaml"])`. The binary path comes from `CARGO_BIN_EXE_yunta`,
/// which Cargo sets only for the tests of the crate that builds the binary
/// — so `env!` is expanded here, at the call site, where that variable
/// exists.
#[macro_export]
macro_rules! yunta_in {
    ($dir:expr, $home:expr, $args:expr) => {
        $crate::run_yunta(
            ::std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
            $dir,
            $home,
            $args,
        )
    };
}

/// Runs the `yunta` binary on a real terminal from an integration test:
/// `yunta_on_terminal!(repo, home, &["run", "wf.yaml"])`. Same reason as
/// [`yunta_in!`] for expanding `CARGO_BIN_EXE_yunta` at the call site.
#[macro_export]
macro_rules! yunta_on_terminal {
    ($dir:expr, $home:expr, $args:expr) => {
        $crate::Terminal::open(
            ::std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
            $dir,
            $home,
            $args,
        )
    };
}
