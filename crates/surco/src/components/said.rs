//! What a message says when a block of its own says it whole: its lines as
//! its author broke them, or a heading and each path it names.

use std::path::PathBuf;

/// Lines said whole, as their author broke them: a message that runs over
/// more than one line, which a surface neither wraps nor joins.
pub struct Said(pub Vec<String>);

/// A path a message names, and the reason it is named for, when the
/// message gives one.
pub struct PathNote {
    pub path: PathBuf,
    pub reason: Option<String>,
}

/// A heading, and each path it names.
pub struct Paths {
    pub heading: String,
    pub paths: Vec<PathNote>,
}
