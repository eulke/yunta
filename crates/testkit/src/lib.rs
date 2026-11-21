//! Shared test harness for the Yunta workspace.
//!
//! One canonical copy of every piece of scaffolding a whole run takes —
//! a git-repo fixture, a binary runner, a pty a run is driven on, a run
//! bench, gate doubles, a recording run
//! observer — so a change to the shape of a test run happens in one
//! place, and so the fixtures are hermetic by construction (git isolated
//! from the developer's global config, a fixed clock, an injected home
//! and terminal) rather than by each test remembering to be.
//!
//! What a test needs below a run — the clocks, the id source, the
//! capturing writer, a session request — is `yunta-testkit-core`, which
//! the crates with no run to build depend on directly.
//!
//! Its map: `bench` is the canonical run harness; `bin` runs the compiled
//! binary, and `checkout` and `child` hold a test that drives it; `held` holds
//! a run on its first node until the test lets it go, and gives the words a
//! held mock session waits for; `workflows` are the
//! workflows tests in more than one file run; `terminal`
//! drives a run on a real terminal; `repo` is a hermetic git fixture and
//! `stubs` stand in for programs on `PATH`; `reference_pack` is the Yunta
//! repository's pack a setup installs, in a local repository git is pointed at
//! for the Yunta repository's URL; `events` reads and writes a run's
//! log, reads where each run of a store stands, loads a log into a store of
//! its own, and sets a store's log aside and puts it back; `registry` writes what a run's
//! registry says of the engine driving it; `interaction` and
//! `observer` are doubles for a gate and the display boundary; `tools` hosts
//! the per-run MCP listener and makes what every run's directory is born with;
//! `said` reads what a run's sessions said off its
//! socket; `tasks` writes a tasks document; `corpus` reads
//! the repository's own documents; `owner` is what a test's subprocesses
//! answer to; `stack` gives a deep run its thread; and `wait` bounds a wait on
//! another process.
//!
//! It is a dev-dependency only: nothing here ships in a published crate.

mod bench;
mod bin;
mod checkout;
mod child;
mod corpus;
mod events;
mod held;
mod interaction;
mod observer;
mod owner;
pub mod reference_pack;
mod registry;
mod repo;
pub mod said;
mod stack;
pub mod stubs;
mod tasks;
mod terminal;
mod tools;
mod wait;
pub mod workflows;

pub use bench::{no_secrets, Bench, MOCK_CONFIG};
pub use bin::{
    checked, checks, full_run_id, handle, hermetic, hung, pool_checkouts, pool_checkouts_in,
    run_id_from, run_ids, run_yunta, stderr, stdout, unwrapped, Spawning,
};
pub use checkout::Checkout;
pub use child::{CliChild, ProcessGroupCleanup};
pub use corpus::{
    backticked, fenced_blocks, has_top_level_key, json_schema, markdown_files, section, Block,
};
pub use events::{
    accepted, baselines, heads, loaded, put_log_back, set_log_aside, status_changed_carrying,
    task_registered, task_status_changed, SourceLog,
};
pub use held::{HeldRun, Hold};
pub use interaction::{ApproveEverything, ScriptedInteraction};
pub use observer::{Frame, RecordingObserver};
pub use owner::Owner;
pub use registry::WrittenRegistry;
pub use repo::{bare_origin, git, git_output, git_with, init_repo, read, write, INITIAL_BRANCH};
pub use stack::on_a_deep_stack;
pub use tasks::tasks_document;
pub use terminal::screen::Screen;
pub use terminal::{runs_root, Terminal};
pub use tools::{born_dirs, tools_environment, ToolsHost};
pub use wait::{after, wait_for, wait_for_async, wait_until, wait_until_async, WAIT_DEADLINE};

/// Runs the `yunta` binary in a [`Checkout`], with whatever that
/// checkout says about its state root and its org layer:
/// `yunta_at!(checkout, &["run", "wf.yaml"])`. The binary path comes from
/// `CARGO_BIN_EXE_yunta`, which Cargo sets only for the tests of the
/// crate that builds the binary — so `env!` is expanded here, at the call
/// site, where that variable exists.
#[macro_export]
macro_rules! yunta_at {
    ($checkout:expr, $args:expr) => {
        $checkout.run(::std::path::Path::new(env!("CARGO_BIN_EXE_yunta")), $args)
    };
}

/// Runs the `yunta` binary from an integration test: `yunta_in!(dir, home,
/// &["run", "wf.yaml"])`. `yunta_in!(dir, home, &["run"], &vars)` sets `vars`
/// on the command as well, after the hermetic environment, which leaves
/// `PATH` alone, so a `PATH` in `vars` is the one the binary runs with. The
/// binary path comes from `CARGO_BIN_EXE_yunta`, which Cargo sets only for
/// the tests of the crate that builds the binary — so `env!` is expanded
/// here, at the call site, where that variable exists.
#[macro_export]
macro_rules! yunta_in {
    ($dir:expr, $home:expr, $args:expr) => {
        $crate::run_yunta(
            ::std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
            $dir,
            $home,
            $args,
            &[],
        )
    };
    ($dir:expr, $home:expr, $args:expr, $vars:expr) => {
        $crate::run_yunta(
            ::std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
            $dir,
            $home,
            $args,
            $vars,
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
    ($dir:expr, $home:expr, $args:expr, $vars:expr) => {
        $crate::Terminal::open_with(
            ::std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
            $dir,
            $home,
            $args,
            $vars,
        )
    };
}

/// Runs the `yunta` binary on a terminal of a stated size, `(columns, rows)`:
/// `yunta_on_terminal_sized!(dir, home, &["review"], (90, 11))`. Same reason as
/// [`yunta_in!`] for expanding `CARGO_BIN_EXE_yunta` at the call site.
#[macro_export]
macro_rules! yunta_on_terminal_sized {
    ($dir:expr, $home:expr, $args:expr, $size:expr) => {
        $crate::Terminal::open_sized(
            ::std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
            $dir,
            $home,
            $args,
            &[],
            $size,
        )
    };
}
