//! What every surface's tests of Yunta's application share, so the CLI's tests, the
//! interface's own and the terminal application's work in one world, drive the
//! application one way and walk the same states.
//!
//! - [`sandbox`]: the world a test works in: a workspace in its own directory that reads
//!   nothing of the machine's, the CLIs it stands in for, packs to install, and the
//!   instant every test stands at.
//! - [`driving`]: a state driven with its performer until it waits on nothing.
//! - [`home`]: the places the walk's recordings are made in.
//! - `planned`: a run parked on its plan's gate, which the screens' world and the home
//!   each make.
//! - [`recorded`]: each kind of work the application asks for, performed once where it
//!   reads something different, and what it found and came to.
//! - [`walk`]: every state the application reaches, answered from the recordings, each
//!   held by the [`walk::Checks`] of a surface; `terminal` is the terminal of a few rows
//!   it lays each screen out on, so every page scrolls.
//! - [`screens`]: the states the terminal application's target frames show, the same on
//!   every machine.

pub mod driving;
pub mod home;
mod planned;
pub mod recorded;
pub mod sandbox;
pub mod screens;
mod terminal;
pub mod walk;
