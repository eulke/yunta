//! Code as a reader of a document meets it: a test file folded to what a
//! review judges, and the code a plan writes read against the code that
//! stands where it goes.

mod diff;
mod fold;
mod lex;

pub(crate) use diff::compared;
pub(crate) use fold::folded;
