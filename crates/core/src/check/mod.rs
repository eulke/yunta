//! What `yunta check` finds in a workflow: the data of every refusal and
//! warning it reaches, with no word a person reads.
//!
//! The engine runs the checks and returns these; render says them, and an
//! interface reads them as data. Each item has one path, `core::check::<item>`.
//! [`CheckError`] is what refuses a workflow; [`Composition`], [`Unanswerable`]
//! and [`UntakenRoute`] are the kinds of it a rule of its own finds.
//!
//! Each knows where in the workflow file it points, for a surface that
//! quotes the file.
//!
//! Not the checking: which rule runs where, and over what, is the engine's.

mod composition;
mod error;
mod pointer;
mod unanswerable;
mod untaken_route;

pub use composition::Composition;
pub use error::CheckError;
pub use unanswerable::Unanswerable;
pub use untaken_route::UntakenRoute;
