//! What a reader can type next, each command with what it does; and what a person can type at
//! a shell to do what an act here does, the commands alone.

/// The commands that move on from what a surface showed, one to a row,
/// the commands in a column so the eye runs down them and the glosses
/// beside them.
pub struct Next {
    pub steps: Vec<(String, &'static str)>,
}

/// The commands that do at a shell what an act here does, each on its row, muted, with no
/// heading and no gloss: what the act does is said above them. Not [`Next`], the steps a page
/// suggests with what each does.
pub struct SameAtAShell(pub Vec<String>);
