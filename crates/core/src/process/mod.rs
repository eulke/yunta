//! Running a child process and owning it for as long as it lives.
//!
//! A subprocess this workspace starts is born in its own process group,
//! belongs to whoever opened it, and dies with its whole tree when that
//! owner is killed or dropped; a session's CLI also takes with it what
//! carries its session's mark. [`signal`] is how it is reached, with the
//! kernel's own answer instead of a `kill` binary's exit status, and
//! [`exit`] how its exit is heard the moment it happens;
//! [`process_start`] is how a later, separate process tells the same pid
//! from a recycled one; [`SearchPath`] is where a program started by its name
//! is found, on the environment's `PATH` and never the process's own;
//! [`subprocess`] is the spawn, [`lines`] the
//! line-by-line read of its output, and [`stream`] the guard that keeps
//! the events read from it in protocol order. [`user`] is who this process
//! runs as, which is who may reach what it serves. [`tree`] walks the groups
//! a session's CLI made under its own, which a signal to its group misses,
//! and [`mark`] closes what carries a session's mark once its parent left.
//!
//! It lives here rather than beside any one caller because an adapter
//! running a CLI, the engine running `git`, and the harness running the
//! binary all own their children the same way.

pub mod exit;
pub mod group;
pub mod lines;
pub mod mark;
pub mod process_start;
mod search_path;
pub mod signal;
pub mod stream;
pub mod subprocess;
pub mod tree;
pub mod user;

pub use search_path::SearchPath;
