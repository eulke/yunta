//! Running a child process and owning it for as long as it lives.
//!
//! A subprocess this workspace starts is born in its own process group,
//! belongs to whoever opened it, and dies with its whole tree when that
//! owner is killed or dropped. [`signal`] is how it is reached, with the
//! kernel's own answer instead of a `kill` binary's exit status;
//! [`process_start`] is how a later, separate process tells the same pid
//! from a recycled one; [`subprocess`] is the spawn and the line-by-line
//! read of its output.
//!
//! It lives here rather than beside any one caller because an adapter
//! running a CLI, the engine running `git`, and the harness running the
//! binary all own their children the same way.

pub mod process_start;
pub mod signal;
pub mod subprocess;
