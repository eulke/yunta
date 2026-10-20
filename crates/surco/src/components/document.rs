//! The blocks a document is built from beyond a surface's own: a titled
//! section of other blocks, a paragraph, a list whose items each carry
//! the same mark, and a file's code.

use std::path::Path;

use crate::doc::Block;
use crate::glyphs::Mark;
use crate::line::Line;

/// What follows a title, one step under it: a medium draws the title
/// and then each block the way it draws any other.
pub struct Section {
    /// What the title is marked with, when its subject carries a mark —
    /// a finding's severity.
    pub mark: Option<Mark>,
    pub title: Line,
    pub blocks: Vec<Block>,
}

/// A paragraph, wrapped to the line by whoever draws it.
pub struct Prose(pub String);

/// Items that each carry the same mark: the risks a plan runs, what it
/// leaves out.
pub struct Marked {
    pub mark: Mark,
    pub items: Vec<String>,
}

/// A file's code, or what of it a reader is shown: where it is, what it
/// is there for, and its lines as written.
pub struct Code {
    /// Where the code is: a file, and what in it.
    pub at: String,
    /// What the code is there for, in words.
    pub what: Option<String>,
    /// The lines shown, as written.
    pub lines: Vec<String>,
    /// How many lines the whole file has, when more than are shown.
    pub whole: usize,
    /// Where the rest is read, when not every line is shown.
    pub rest: Option<String>,
}

impl Code {
    /// `code` whole, from the file at `at`.
    pub fn whole(at: impl Into<String>, what: Option<String>, code: &str) -> Self {
        let lines: Vec<String> = code.trim_end().lines().map(str::to_string).collect();
        Code {
            at: at.into(),
            what,
            whole: lines.len(),
            lines,
            rest: None,
        }
    }

    /// `code`, the file at `at`, as a review shows it: each test whole and
    /// each function that only sets one up folded to its signature, at
    /// most `shown` lines, and where the whole file is whenever any of it
    /// is hidden. Its size is the file's own.
    pub fn reviewed(
        at: impl Into<String>,
        what: Option<String>,
        code: &str,
        shown: usize,
        whole_at: impl Into<String>,
    ) -> Self {
        let at = at.into();
        let whole_at = whole_at.into();
        let folded = crate::code::folded(&at, code);
        let mut reviewed = Code::whole(at, what, &folded).cut(shown, whole_at.clone());
        reviewed.whole = code.trim_end().lines().count();
        if reviewed.whole > reviewed.lines.len() {
            reviewed.rest.get_or_insert(whole_at);
        }
        reviewed
    }

    /// `code` with at most `shown` of its lines, and where the rest is.
    pub fn cut(mut self, shown: usize, rest: impl Into<String>) -> Self {
        if self.lines.len() > shown {
            self.lines.truncate(shown);
            self.rest = Some(rest.into());
        }
        self
    }

    /// The language a fence names for the file the code is in, by its
    /// extension, so a viewer that colors code colors it right.
    pub fn language(&self) -> &str {
        let extension = self
            .at
            .split(|c: char| c.is_whitespace() || c == ':')
            .find_map(|part| Path::new(part).extension().and_then(|ext| ext.to_str()));
        match extension {
            Some("rs") => "rust",
            Some("py") => "python",
            Some("ts") => "typescript",
            Some("js") => "javascript",
            Some("go") => "go",
            Some("yaml" | "yml") => "yaml",
            Some("toml") => "toml",
            Some("sh") => "sh",
            Some("md") => "markdown",
            _ => "",
        }
    }
}
