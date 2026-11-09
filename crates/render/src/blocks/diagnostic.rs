//! Problems found in a text, each placed in it: the blocks a refused
//! document and a workflow that fails `yunta check` are drawn with, and
//! the same problems said as plain text to a reader that reads no page.
//! A problem that names a key or a kind nothing in a workflow reads is
//! [`Pointed`] at it, so the command that lists what a workflow reads is
//! said once, under the first of them ([`hinted`]).

use surco::components::{Place, Problem};
use surco::doc::Doc;
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::diagnostic::{Diagnostic, RuleCode};
use yunta_core::step::{Arg, NextStep};
use yunta_core::yaml::Location;

/// What is wrong at `at` in `text`, read from `path`, as an error: its line
/// quoted when the text has one there, said alone when it has no place.
pub fn located(said: String, at: Option<Location>, path: &str, text: Option<&str>) -> Problem {
    Problem::error(
        said,
        at.map(|at| Place {
            path: path.to_string(),
            line: at.line,
            col: at.col,
            source: text
                .and_then(|text| text.lines().nth(at.line.saturating_sub(1)))
                .map(str::to_string),
            len: at.len,
        }),
    )
}

/// One problem a document read from `path` has: what it says, quoted from
/// `text` at its place when the document was read from text, and what
/// helps fix it.
pub fn found(diagnostic: &Diagnostic, path: &str, text: Option<&str>) -> Problem {
    let problem = match (text, diagnostic.at) {
        (Some(_), Some(at)) => located(diagnostic.said(), Some(at), path, text),
        _ => located(diagnostic.said_unplaced(), None, path, text),
    };
    problem.help(diagnostic.help())
}

/// `problems` under `heading`, counted, as plain text says them — to an
/// agent, in a log: each drawn the way a terminal with neither color nor
/// Unicode draws it.
pub fn plain(heading: &str, problems: Vec<Problem>) -> String {
    let counted = counted_under(heading, problems.len());
    let doc = problems.into_iter().fold(Doc::new(), Doc::with);
    let drawn = Terminal::on(Look::plain()).draw(&doc);
    format!("{counted}\n{}", drawn.trim_end())
}

/// `heading` and how many problems are under it: the line a refusal of
/// them opens with, for a person and an agent alike.
pub fn counted_under(heading: &str, problems: usize) -> String {
    format!("{heading}: {}", surco::text::counted(problems, "error"))
}

/// A problem a page says, and what it names that nothing in a workflow
/// reads, when that is what it is about.
pub struct Pointed {
    problem: Problem,
    unread: Option<Unread>,
}

/// What a problem can name that nothing in a workflow reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unread {
    Key,
    Kind,
}

impl Unread {
    /// The word the hint lists it by.
    fn word(self) -> &'static str {
        match self {
            Unread::Key => "key",
            Unread::Kind => "kind",
        }
    }
}

impl From<Problem> for Pointed {
    fn from(problem: Problem) -> Self {
        Pointed {
            problem,
            unread: None,
        }
    }
}

impl Pointed {
    /// One problem of a document read from `path`, quoted from `text` at its
    /// place when it was read from text, with what helps fix it.
    pub fn found(diagnostic: &Diagnostic, path: &str, text: Option<&str>) -> Self {
        let unread = match diagnostic.rule() {
            Some(RuleCode::UnknownKey) => Some(Unread::Key),
            Some(RuleCode::UnknownKind) => Some(Unread::Kind),
            _ => None,
        };
        Pointed {
            problem: found(diagnostic, path, text),
            unread,
        }
    }

    /// Where in its file the problem is, as a page orders them: what has
    /// no place comes after everything that has one.
    pub fn order(&self) -> (usize, usize) {
        self.problem
            .at
            .as_ref()
            .map_or((usize::MAX, 0), |at| (at.line, at.col))
    }
}

/// `problems` as a page says them, in their order: the command that lists
/// what a workflow reads is said once, under the first that names a key or
/// a kind nothing reads, with what every one of them needs listed.
pub fn hinted(problems: Vec<Pointed>) -> Vec<Problem> {
    let mut unread: Vec<&str> = Vec::new();
    for what in problems
        .iter()
        .filter_map(|pointed| pointed.unread)
        .map(Unread::word)
    {
        if !unread.contains(&what) {
            unread.push(what);
        }
    }
    let mut hint = (!unread.is_empty()).then(|| {
        format!(
            "`{}` lists every {}",
            crate::command::line(&NextStep::Schema {
                document: Arg::Given(yunta_core::DocumentKind::Workflow.as_str().to_string()),
                json: false,
            }),
            unread.join(" and ")
        )
    });
    problems
        .into_iter()
        .map(|pointed| match pointed.unread {
            Some(_) => pointed.problem.help(hint.take()),
            None => pointed.problem,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_problem_keeps_the_line_it_is_about() {
        let text = "nodes:\n  - id: fix\n    runner: implementr\n";
        let at = Location {
            line: 3,
            col: 13,
            len: 10,
        };
        let problem = located("no runner".to_string(), Some(at), "wf.yaml", Some(text));
        let place = problem.at.expect("a place");
        assert_eq!(place.named(), "wf.yaml:3:13");
        assert_eq!(place.source.as_deref(), Some("    runner: implementr"));
    }

    #[test]
    fn a_place_past_the_end_of_the_text_quotes_nothing() {
        let at = Location {
            line: 9,
            col: 1,
            len: 1,
        };
        let problem = located("x".to_string(), Some(at), "wf.yaml", Some("a: 1\n"));
        assert_eq!(problem.at.and_then(|place| place.source), None);
    }

    #[test]
    fn a_diagnostic_is_quoted_at_its_place_with_what_helps_fix_it() {
        let text = "name: w\nnodes: []\nnodez: 1\n";
        let (_, report) =
            yunta_core::workflow::read::read_all(text, std::path::Path::new("w.yaml"));
        let diagnostic = &report.diagnostics[0];
        let quoted = found(diagnostic, "w.yaml", Some(text));
        assert_eq!(quoted.said, "the document has an unknown key `nodez`");
        assert_eq!(quoted.help, ["did you mean `nodes`?"]);
        let place = quoted.at.expect("a place");
        assert_eq!(
            (place.named(), place.source.as_deref()),
            ("w.yaml:3:1".to_string(), Some("nodez: 1"))
        );
        let unquoted = found(diagnostic, "w.yaml", None);
        assert!(unquoted.at.is_none());
        assert_eq!(
            unquoted.said,
            "the document has an unknown key `nodez` at line 3 column 1"
        );
        assert_eq!(unquoted.help, ["did you mean `nodes`?"]);
    }

    #[test]
    fn plain_text_counts_the_problems_and_quotes_each_line() {
        let text = "nodes:\n  - id: fix\n    runner: implementr\n";
        let at = Location {
            line: 3,
            col: 13,
            len: 10,
        };
        let problems = vec![located(
            "node `fix` references runner `implementr`".to_string(),
            Some(at),
            "wf.yaml",
            Some(text),
        )];
        assert_eq!(
            plain("wf.yaml", problems),
            "wf.yaml: 1 error\n  error  node `fix` references runner `implementr`\n   --> \
             wf.yaml:3:13\n    |\n  3 |     runner: implementr\n    |             ^^^^^^^^^^"
        );
    }
}
