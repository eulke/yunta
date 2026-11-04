//! A caution a reader acts on: what is so, what it means for what they
//! decide, and what fixes it.

use super::Fix;

/// What stands against a decision: the fact, what it means, and what
/// fixes it — a caution that names no way out leaves the reader with a
/// worry rather than a choice.
pub struct Concern {
    pub fact: String,
    pub so: String,
    pub fix: Fix,
}
