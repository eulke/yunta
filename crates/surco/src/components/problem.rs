//! A problem found in a text and the place it is about, the way a
//! compiler points at a line: how grave it is, what it says, the line as
//! written with the part at fault marked under it, and what helps fix it.

/// How grave a problem is: an error stops what was asked, and a warning
/// is worth hearing before it goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// The word a problem opens with.
    pub fn word(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One problem: how grave, what it says, where, when it has a place, and
/// what helps fix it, the most direct first.
#[derive(Debug, Clone)]
pub struct Problem {
    pub severity: Severity,
    pub said: String,
    pub at: Option<Place>,
    pub help: Vec<String>,
}

impl Problem {
    /// An error that says `said`, at `at` when it has a place, with nothing
    /// to help fix it yet.
    pub fn error(said: impl Into<String>, at: Option<Place>) -> Self {
        Problem {
            severity: Severity::Error,
            said: said.into(),
            at,
            help: Vec::new(),
        }
    }

    /// A warning that says `said`, with no place.
    pub fn warning(said: impl Into<String>) -> Self {
        Problem {
            severity: Severity::Warning,
            ..Problem::error(said, None)
        }
    }

    /// This problem with `help` said under it, after what it has.
    pub fn help(mut self, help: impl IntoIterator<Item = String>) -> Self {
        self.help.extend(help);
        self
    }
}

/// Where in a text a problem is: the file as its reader names it, the line
/// and the column, each counted from one, the line as written when the
/// text has it, and how many characters from the column are at fault.
#[derive(Debug, Clone)]
pub struct Place {
    pub path: String,
    pub line: usize,
    pub col: usize,
    pub source: Option<String>,
    pub len: usize,
}

impl Place {
    /// Where it is, as a reader types it into an editor.
    pub fn named(&self) -> String {
        format!("{}:{}:{}", self.path, self.line, self.col)
    }

    /// The line as written with a tab four cells wide, so the line and the
    /// marks under it agree on where the fault is; the part before the
    /// fault; and how many characters of it are at fault, one at least.
    pub fn fault(&self) -> Option<(String, String, usize)> {
        let source = self.source.as_ref()?.replace('\t', "    ");
        let before: String = source.chars().take(self.col.saturating_sub(1)).collect();
        let rest = source
            .chars()
            .count()
            .saturating_sub(before.chars().count());
        let len = self.len.clamp(1, rest.max(1));
        Some((source, before, len))
    }
}

/// What a command would not do, said whole, and where it looked when that
/// is what a reader checks next. The problems it found and the commands
/// that go on from here are blocks of their own beside it.
#[derive(Debug, Clone)]
pub struct Refusal {
    pub said: String,
    pub detail: Option<String>,
}
