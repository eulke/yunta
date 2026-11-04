//! The blocks a document is built from beyond a surface's own: a titled
//! section of other blocks, a paragraph, a list whose items each carry
//! the same mark, and a file's code, with what it is to the code that
//! stands where it goes when a reader is told.

use crate::doc::Block;
use crate::glyphs::Mark;
use crate::line::Line;

/// What a whole document is, before anything it says: its name, and the
/// facts it states beside it. A line too narrow for all of them breaks
/// between two facts, never inside one, and goes on under the first fact.
pub struct Title {
    pub name: Line,
    pub facts: Vec<String>,
}

impl Title {
    /// A title that is its name alone.
    pub fn named(name: Line) -> Self {
        Title {
            name,
            facts: Vec::new(),
        }
    }
}

/// A section a reader opens when they want what it holds: a task's card
/// under its step, a proof under its heading. A surface that folds draws
/// it closed under its title; every other surface draws it as the section
/// it is.
pub struct Fold(pub Section);

/// What follows a title, one step under it: a medium draws the title
/// and then each block the way it draws any other.
pub struct Section {
    /// What the title is led by, when anything leads it.
    pub lead: Option<Lead>,
    pub title: Line,
    pub blocks: Vec<Block>,
}

/// What leads a section's title: the mark its subject carries — a
/// finding's severity — or its place in a numbered list. A title too long
/// for its line goes on past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lead {
    Mark(Mark),
    Ordinal(usize),
}

/// A paragraph, wrapped to the line by whoever draws it.
pub struct Prose(pub String);

/// A remark beside what a page says: true and worth saying, and not what
/// its reader came for. A terminal says it muted, a file in italics.
pub struct Note(pub String);

/// Items that each carry the same mark: the risks a plan runs, what it
/// leaves out.
pub struct Marked {
    pub mark: Mark,
    pub items: Vec<String>,
}

/// A file's code, or what of it a reader is shown: where it is, what it
/// is there for, and its lines as written.
pub struct Code {
    /// The file the code is in.
    pub at: String,
    /// What in the file the code is, outermost first: a type, then a
    /// function of it.
    pub within: Vec<String>,
    /// What the code is there for, in words.
    pub what: Option<String>,
    /// The lines shown, as written.
    pub lines: Vec<String>,
    /// How many lines the whole file has, when more than are shown.
    pub whole: usize,
    /// Where the rest is read, when not every line is shown.
    pub rest: Option<String>,
    /// What the code is to the code that stands where it goes today, when a
    /// reader is told: said under its lines with its size, and marked in its
    /// words where it modifies that code.
    pub against: Option<Against>,
}

/// What code a reader is shown is to the code that stands where it goes
/// today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Against {
    /// Nothing stands there: all of it is new.
    New,
    /// What stands there reads the same, word for word.
    Unchanged,
    /// It changes what stands there: its lines with the words it puts in and
    /// takes out marked, or `None` where the two are too long to compare
    /// word by word.
    Modified(Option<Marks>),
    /// Something of its name stands there, in code that is not compared.
    Uncompared,
}

impl Against {
    /// The words a reader is told it in: `new`, `unchanged`, `modified`,
    /// `modified, too long to mark`, `exists, not compared`.
    pub fn word(&self) -> &'static str {
        match self {
            Against::New => "new",
            Against::Unchanged => "unchanged",
            Against::Modified(Some(_)) => "modified",
            Against::Modified(None) => "modified, too long to mark",
            Against::Uncompared => "exists, not compared",
        }
    }
}

/// Code read against the code it changes, word by word: each of its lines
/// as the parts it reads in, one entry a line of the code's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marks {
    pub lines: Vec<Vec<Part>>,
}

/// A run of a modified line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Text the code keeps of today's, or what stands between words.
    Kept(String),
    /// Words the code puts in, with what stands between them.
    Added(String),
    /// Words of today's code it takes out, where they stood.
    Removed(String),
}

impl Code {
    /// `code` whole, from the file at `at`.
    pub fn whole(at: impl Into<String>, what: Option<String>, code: &str) -> Self {
        let lines: Vec<String> = code.trim_end().lines().map(str::to_string).collect();
        Code {
            at: at.into(),
            within: Vec::new(),
            what,
            whole: lines.len(),
            lines,
            rest: None,
            against: None,
        }
    }

    /// This code read against `today`, the code that stands where it goes,
    /// word by word in its file's language; `None` where nothing stands
    /// there, which makes all of it new. It reads the lines the code shows,
    /// so it is told of code shown whole.
    pub fn against(mut self, today: Option<&str>) -> Self {
        self.against = Some(match today {
            None => Against::New,
            Some(today) => crate::code::compared(&self.at, &self.lines.join("\n"), today),
        });
        self
    }

    /// This code beside something of its name that stands where it goes,
    /// which is not compared.
    pub fn uncompared(mut self) -> Self {
        self.against = Some(Against::Uncompared);
        self
    }

    /// What is said under the code's lines, a part each: what it is to the
    /// code that stands where it goes, when told, and how many lines it has.
    /// A surface joins them with its separator.
    pub fn footer(&self) -> Vec<String> {
        let against = self
            .against
            .as_ref()
            .map(|against| against.word().to_string());
        let size = (self.whole > 0).then(|| crate::text::counted(self.whole, "line"));
        against.into_iter().chain(size).collect()
    }

    /// This code, as what `within` names inside its file.
    pub fn within(mut self, within: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.within = within.into_iter().map(Into::into).collect();
        self
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
        crate::code::fence(&self.at)
    }
}
