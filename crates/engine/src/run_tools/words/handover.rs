//! The sentences a handover gives a writer that the engine composes itself.
//! The clause of each rule a handover breaks is written from its fault in
//! core, beside the rule, so what is left here is what a failed write says.

use std::fmt::Display;

/// What a handover says when a test file could not be written into the
/// checkout its tests run in.
pub(crate) fn could_not_write(path: &str, source: impl Display) -> String {
    format!("could not write `{path}`: {source}")
}
