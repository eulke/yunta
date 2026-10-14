//! The one owner of what a command writes: the mode its output takes, the
//! answer it hands back, the text it writes as it goes, and the refusal
//! `main` writes when it cannot give one.
//!
//! A command never picks a mode of its own. `main` reads the mode from the
//! flags once; a command returns an [`Answer`], and [`emit`] writes it the
//! way that mode asks. A refusal goes through [`refused`]: a person reads it
//! on stderr, and a program that asked for `--json` reads one document on
//! stdout, with stderr left empty.

use crate::error::{note, CliError, Outcome, Reader};
use crate::render::doc::{Block, Doc};
use crate::render::ink::Line;
use crate::render::{self, Look};

/// How a command's output is written, decided once from its flags: a
/// person's terminal, one JSON document, or the bare run id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Human,
    Json,
    Quiet,
}

impl Mode {
    /// `--json` and `--quiet` conflict at parse time, so at most one is set.
    pub(crate) fn of(json: bool, quiet: bool) -> Self {
        match (json, quiet) {
            (true, _) => Mode::Json,
            (false, true) => Mode::Quiet,
            (false, false) => Mode::Human,
        }
    }
}

/// What a command answers. The command returns it; [`emit`] writes it.
pub(crate) trait Answer: serde::Serialize {
    /// The answer as the document a person reads, drawn on a stream that
    /// looks like `look`.
    fn document<'a>(&'a self, look: &Look) -> Doc<'a>;
    /// The verdict the answer carries, which the exit code reports.
    fn outcome(&self) -> Outcome;
}

/// Writes `answer` the way `mode` asks, and hands back its verdict.
pub(crate) fn emit<A: Answer>(mode: Mode, answer: &A, look: &Look) -> Result<Outcome, CliError> {
    match mode {
        Mode::Json => {
            crate::json::print_json(answer)?;
        }
        Mode::Human => document(look, answer.document(look)),
        // `--quiet` says the run id and nothing else, and an answer has
        // nothing to add to that.
        Mode::Quiet => {}
    }
    Ok(answer.outcome())
}

/// A line a person waits on, written as it happens. A `--json` reader gets
/// its one document at the end, so the line is for a person's terminal only.
pub(crate) fn progress(mode: Mode, look: &Look, text: String) {
    if mode == Mode::Human {
        say(look, text);
    }
}

/// A document drawn for a stream that looks like `look`, and written.
pub(crate) fn document(look: &Look, doc: Doc<'_>) {
    print!("{}", render::draw(doc, look));
}

/// One line written as it is decided, drawn the way a document draws its
/// lines. A command that writes lines as it goes has no document to hold them.
pub(crate) fn say(look: &Look, text: impl Into<String>) {
    document(
        look,
        Doc::new().with(Block::Lines(vec![Line::new().plain(text)])),
    );
}

/// An empty line. A document draws nothing ahead of the first line it says,
/// so the blank line that separates two blocks is written here.
pub(crate) fn blank() {
    println!();
}

/// Text written exactly as it is: no line end is added and nothing is drawn
/// on it. For text a command renders itself, such as a diagram's source or
/// a schema.
pub(crate) fn raw(text: &str) {
    print!("{text}");
}

/// A refusal as a program reads it under `--json`: one document, and the
/// version of the schema it was written under.
#[derive(serde::Serialize)]
pub(crate) struct ErrorDocument {
    schema_version: u32,
    error: ErrorBody,
}

/// What a refusal says, where it looked, and the commands that go on from
/// here.
#[derive(serde::Serialize)]
pub(crate) struct ErrorBody {
    said: String,
    detail: Option<String>,
    next: Vec<NextCommand>,
}

/// A command a person or a program can type next, and what it is for.
#[derive(serde::Serialize)]
pub(crate) struct NextCommand {
    command: String,
    why: String,
}

impl ErrorDocument {
    /// The refusal `error` is, as `reader` is told it.
    pub(crate) fn of(error: &CliError, reader: Reader) -> Self {
        let (detail, next) = error.advice();
        Self {
            schema_version: crate::json::SCHEMA_VERSION,
            error: ErrorBody {
                said: error.said_to(reader),
                detail,
                next: next
                    .into_iter()
                    .map(|(command, why)| NextCommand {
                        command,
                        why: why.to_string(),
                    })
                    .collect(),
            },
        }
    }
}

/// Writes the refusal `error` for a command run in `mode`. Under `--json`
/// it is the one document on stdout, and stderr stays empty. Otherwise a
/// person reads the sentence on stderr, then where the refusal looked and
/// the commands that go on from here.
pub(crate) fn refused(error: &CliError, mode: Mode, look: &Look) {
    if mode == Mode::Json {
        // The whole id, which is what a program copies from a refusal.
        let refusal = ErrorDocument::of(error, Reader::Agent);
        if let Err(problem) = crate::json::print_json(&refusal) {
            note(problem);
        }
        return;
    }
    // What a person reads on stderr is painted and measured for stderr,
    // whatever stdout is: a stream redirected away from the terminal does
    // not decide how the refusal on the terminal looks.
    let ink = render::stderr_ink();
    eprintln!(
        "{}: {}",
        ink.word(render::ink::Tone::Failed, "error"),
        error.said_to(Reader::Person)
    );
    let (detail, next) = error.advice();
    if let Some(detail) = detail {
        eprintln!(
            "{}{}",
            render::INDENT,
            ink.word(render::ink::Tone::Muted, &detail)
        );
    }
    if !next.is_empty() {
        let stderr = Look {
            ink,
            width: render::stderr_width(),
            ..*look
        };
        eprint!(
            "\n{}",
            render::draw(
                Doc::new().with(render::blocks::Next { steps: next }),
                &stderr
            )
        );
    }
}
