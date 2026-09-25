//! Shared test harness for the Yunta workspace.
//!
//! One canonical copy of every piece of scaffolding a whole run takes —
//! a git-repo fixture, a binary runner, a pty a run is driven on, a run
//! bench, the frames a surface draws, gate doubles, a recording run
//! observer — so a change to the shape of a test run happens in one
//! place, and so the fixtures are hermetic by construction (git isolated
//! from the developer's global config, a fixed clock, an injected home
//! and terminal) rather than by each test remembering to be.
//!
//! What a test needs below a run — the clocks, the id source, the
//! capturing writer, a session request — is `yunta-testkit-core`, which
//! the crates with no run to build depend on directly.
//!
//! It is a dev-dependency only: nothing here ships in a published crate.

mod bench;
mod bin;
mod checkout;
mod child;
mod corpus;
mod events;
mod frames;
mod interaction;
mod observer;
mod owner;
mod repo;
mod stack;
pub mod stubs;
mod tasks;
mod terminal;
mod tools;
mod wait;

pub use bench::{Bench, MOCK_CONFIG};
pub use bin::{hermetic, run_id_from, run_id_in, run_yunta, stderr, stdout, Spawning};
pub use checkout::Checkout;
pub use child::{force_kill_process_group, CliChild};
pub use corpus::{
    backticked, bullets, fenced_blocks, field_tables, fixed_consts, has_top_level_key, json_schema,
    markdown_files, names_after, number_before, numbered_items, rule_codes_named, section,
    sentence_after, struct_fields, table_rows, tagged_variants, Block,
};
pub use events::{
    accepted, baselines, status_changed_carrying, task_registered, task_status_changed, SourceLog,
};
pub use frames::{child_link, moment, node_frame, run_frame};
pub use interaction::{ApproveEverything, ScriptedInteraction};
pub use observer::{Frame, RecordingObserver};
pub use owner::Owner;
pub use repo::{git, git_output, init_repo, read, write, INITIAL_BRANCH};
pub use stack::on_a_deep_stack;
pub use tasks::tasks_document;
pub use terminal::{runs_root, Terminal};
pub use tools::ToolsHost;
pub use wait::{wait_for, wait_for_async, wait_until, wait_until_async, WAIT_DEADLINE};

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
