//! What a reader can type next, each command with what it does.

/// The commands that move on from what a surface showed, one to a row,
/// the commands in a column so the eye runs down them and the glosses
/// beside them.
pub struct Next {
    pub steps: Vec<(String, &'static str)>,
}
