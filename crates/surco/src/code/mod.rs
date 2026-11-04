//! Code as a reader of a document meets it: one reading of its comments,
//! literals and words in each language it knows, shared by every reader of
//! code, and what each word is, which colors it; one reading of the items a
//! text declares, in Rust, Go, JavaScript or TypeScript, and Python, which
//! the engine and the fold of a test file share; a test file folded to what
//! a review judges; and the code a plan writes read against the code that
//! stands where it goes.

mod diff;
mod fold;
mod highlight;
pub mod items;
mod lex;

pub(crate) use diff::compared;
pub(crate) use fold::folded;
pub(crate) use highlight::{highlighted, painted};
pub(crate) use lex::{code_spans, fence_left_open, heading_level, Fences, MIN_FENCE};
pub use lex::{fence, is_word, masked, Language};

/// Whether `word` is one of the words `list` holds, apart by spaces: the way
/// a reader of code keeps a language's lists of words.
fn listed(list: &str, word: &str) -> bool {
    list.split_ascii_whitespace().any(|listed| listed == word)
}
