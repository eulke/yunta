//! A problem found in a text and the place it is about, the way a
//! compiler points at a line: how grave it is, what it says, the line as
//! written with the part at fault marked under it, and what helps fix it.

use crate::glyphs::InAscii;

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
    /// The column the fault was recorded at, as an editor counts it: what [`Place::named`]
    /// says, wherever the line's text took another form.
    pub col: usize,
    pub source: Option<String>,
    pub len: usize,
    /// Where the marks under the quoted line start and how many there are, in characters of
    /// the line with its tabs four cells wide, where its text took a form of another length
    /// (a line in ASCII): none where they start at `col` and take `len`.
    pub marked: Option<(usize, usize)>,
}

impl Place {
    /// Where it is, as a reader types it into an editor.
    pub fn named(&self) -> String {
        format!("{}:{}:{}", self.path, self.line, self.col)
    }

    /// The line as written with a tab four cells wide, so the line and the
    /// marks under it agree on where the fault is; the part before the
    /// fault; and how many characters of it are at fault, one at least: from
    /// where the marks were moved to, where the line took another form.
    pub fn fault(&self) -> Option<(String, String, usize)> {
        let source = self.source.as_ref()?.replace('\t', "    ");
        let (at, len) = self
            .marked
            .unwrap_or((self.col.saturating_sub(1), self.len));
        let before: String = source.chars().take(at).collect();
        let rest = source
            .chars()
            .count()
            .saturating_sub(before.chars().count());
        let len = len.clamp(1, rest.max(1));
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

/// A place in a quoted line, in ASCII: the line and the path in their ASCII forms, the line
/// and the column it was recorded at kept, as an editor opens them, and the marks under the
/// fault moved to where its ASCII form stands, as many as its ASCII form takes.
impl InAscii for Place {
    fn in_ascii(&self) -> Self {
        let marked = self.fault().map(|(line, before, len)| {
            let at = before.chars().count();
            let fault: String = line.chars().skip(at).take(len).collect();
            let folded = |text: &str| text.to_string().in_ascii().chars().count();
            (folded(&before), folded(&fault).max(1))
        });
        let Place {
            path,
            line,
            col,
            source,
            len,
            marked: _,
        } = self;
        Place {
            path: path.in_ascii(),
            line: *line,
            col: *col,
            source: source.in_ascii(),
            len: *len,
            marked,
        }
    }
}
