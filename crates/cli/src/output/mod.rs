//! The one owner of what a command writes: the mode its output takes, the
//! answer it hands back, the text it writes as it goes, and the refusal
//! `main` writes when it cannot give one.
//!
//! A command never picks a mode of its own. `main` reads the mode from the
//! flags once; a command returns an [`Answer`], and [`emit`] writes it the
//! way that mode asks. A refusal goes through [`refused`]: a person reads it
//! on stderr, and a program that asked for `--json` reads one document on
//! stdout, with stderr left empty.
//!
//! The code the process exits with is here ([`Outcome`], [`exit()`]), and what
//! drawing depends on in this process is here too: the streams and the
//! policy `main` settles once ([`stream`]), and a document laid out or drawn
//! for a stream's look ([`draw`], [`blocks_lines`]).
//!
//! Warnings and notes reach stderr through [`warn`] and [`note`], and a JSON
//! document reaches stdout through [`print_json`]. A shell's completion
//! script, a hook's answer and what clap made of the arguments go through
//! [`completions`], [`hook_reply`] and [`clap_answer`]. Nothing else in the
//! CLI calls a print macro.

use std::fmt::Display;

use surco::doc::{Block, Doc};
use surco::line::Line;
use surco::look::Look;

use crate::error::{warning_word, CliError};
use crate::json::to_json_string;
use yunta_render::command::Reader;
use yunta_render::page::Page;

mod exit;
mod stream;

pub(crate) use exit::{exit, Outcome, RunExit};

pub(crate) use stream::{
    glyphs, row_width, settle_color, settle_columns, settle_glyphs, stderr_look, stderr_paint,
    stderr_width, stdout_look,
};

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

/// What a command answers: the page a person reads, the JSON a program reads
/// (the answer itself, serialized), and the verdict its exit code reports.
/// The command returns it; [`emit`] writes it.
pub(crate) trait Answer: serde::Serialize {
    /// The page a person reads for this answer.
    fn page(&self) -> impl Page + '_;
    /// The verdict the answer carries, which the exit code reports.
    fn outcome(&self) -> Outcome;
}

/// Writes `answer` the way `mode` asks, and hands back its verdict.
pub(crate) fn emit<A: Answer>(mode: Mode, answer: &A, look: &Look) -> Result<Outcome, CliError> {
    match mode {
        Mode::Json => {
            print_json(answer)?;
        }
        Mode::Human => show(look, &answer.page()),
        // `--quiet` says the run id and its verdict, and an answer has nothing
        // to add to that.
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

/// A page drawn for a stream that looks like `look`, and written.
pub(crate) fn show(look: &Look, page: &impl Page) {
    document(look, page.document(look));
}

/// A document drawn for a stream that looks like `look`, and written: what
/// [`show`] writes of a page, and [`say`] of a line.
fn document(look: &Look, doc: Doc) {
    print!("{}", draw(doc, look));
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

/// A shell's completion script, written to stdout as the generator writes it.
pub(crate) fn completions(shell: clap_complete::Shell, command: &mut clap::Command) {
    clap_complete::generate(shell, command, "yunta", &mut std::io::stdout());
}

/// A hook's answer: its stdout, then its stderr, each written exactly as the
/// protocol gives it. The caller reads both streams.
pub(crate) fn hook_reply(stdout: &[u8], stderr: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    std::io::stdout()
        .write_all(stdout)
        .and_then(|()| std::io::stderr().write_all(stderr))
}

/// What clap made of the arguments, as it rendered it, to the stream its
/// kind names: a usage error to stderr, the help and the version to stdout.
pub(crate) fn clap_answer(error: &clap::Error) -> std::io::Result<()> {
    use std::io::Write;
    let rendered = error.render().ansi().to_string();
    match error.use_stderr() {
        true => std::io::stderr().write_all(rendered.as_bytes()),
        false => std::io::stdout().write_all(rendered.as_bytes()),
    }
}

/// Serializes a DTO as pretty JSON to stdout — the one place a `--json`
/// command prints its document, reporting a serialization failure as the
/// one error it can hit rather than unwrapping it.
pub fn print_json<T: serde::Serialize>(value: &T) -> Result<Outcome, CliError> {
    println!("{}", to_json_string(value).map_err(CliError::msg)?);
    Ok(Outcome::Success)
}

/// A warning to stderr — the one place the CLI prints `warning:` lines,
/// so a run that succeeds with caveats still says so without an error.
/// The word is painted in the caution color on a stream that draws one,
/// so a warning said before a run starts is not lost among the lines
/// around it.
pub fn warn(message: impl Display) {
    eprintln!("{}: {message}", warning_word());
}

/// An informational block to stderr — verification findings and the
/// like, printed beside a command's own output without claiming to be an
/// error.
pub fn note(message: impl Display) {
    eprintln!("{message}");
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

impl NextCommand {
    /// A command a reader can type next, with the one line that says what it does.
    pub(crate) fn new(command: String, why: &str) -> Self {
        Self {
            command,
            why: why.to_string(),
        }
    }
}

impl ErrorDocument {
    /// The refusal `error` is, as `reader` is told it.
    pub(crate) fn of(error: &CliError, reader: Reader) -> Self {
        let (detail, next) = error.advice(reader);
        Self {
            schema_version: crate::json::SCHEMA_VERSION,
            error: ErrorBody {
                said: error.said_to(reader),
                detail,
                next: next
                    .into_iter()
                    .map(|(command, why)| NextCommand::new(command, why))
                    .collect(),
            },
        }
    }
}

/// Writes the refusal `error` for a command run in `mode`. Under `--json`
/// it is the one document on stdout, and stderr stays empty. Otherwise a
/// person reads it on stderr as a page: the sentence and where it looked,
/// each problem it found pointed at in its line, and the commands that go
/// on from here.
pub(crate) fn refused(error: &CliError, mode: Mode, look: &Look) {
    if mode == Mode::Json {
        // The whole id, which is what a program copies from a refusal.
        let refusal = ErrorDocument::of(error, Reader::Agent);
        if let Err(problem) = print_json(&refusal) {
            note(problem);
        }
        return;
    }
    show_on_stderr(look, error);
}

/// A page a person reads on stderr: a refusal, or a verdict that fails.
/// It is painted and measured for stderr, whatever stdout is: a stream
/// redirected away from the terminal does not decide how a page on the
/// terminal looks.
pub(crate) fn show_on_stderr(look: &Look, page: &impl Page) {
    let stderr = Look {
        paint: stderr_paint(),
        width: stderr_width(),
        ..*look
    };
    eprint!("{}", draw(page.document(&stderr), &stderr));
}

/// `blocks`, in order, laid out the same way.
pub(crate) fn blocks_lines(blocks: &[Block], look: &Look) -> Vec<Line> {
    surco::surface::Terminal::on(*look).blocks(blocks)
}

/// `doc` drawn on a terminal stream whose lines look like `look`.
pub(crate) fn draw(doc: Doc, look: &Look) -> String {
    use surco::surface::Surface;
    surco::surface::Terminal::on(*look).draw(&doc)
}
