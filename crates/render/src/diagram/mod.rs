//! A diagram an author wrote, as each surface draws it: a file keeps the
//! Mermaid source as written, and a terminal draws what it can read of
//! it.

pub mod flowchart;

pub use flowchart::{flowchart, Direction, Flowchart, Link, Node, Shape, Stroke};
