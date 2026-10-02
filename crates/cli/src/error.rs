//! The one error a command hands back and the one place its text reaches
//! a person. Every subcommand returns [`Result<Outcome, CliError>`]; `main`
//! prints a [`CliError`] once, on stderr, and maps the exit code. A
//! command that ran but reports a failing verdict (an invalid workflow, a
//! test case that failed) returns [`Outcome::Reported`] instead — its
//! detail is already on the command's own output, and no error banner
//! belongs on top of it.

use std::fmt::Display;

use yunta_core::RunId;

/// How a subcommand came back when nothing stopped it from running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The command did its job; exit 0.
    Success,
    /// The command ran and reports a failing verdict — the detail is
    /// already on its output. Exit non-zero, no error banner.
    Reported,
    /// The command's whole answer is its exit code, because the process
    /// that ran it reads one: the fence hook, whose calling CLI takes
    /// `2` as a refusal and `0` as consent.
    Code(u8),
}

/// Everything a subcommand can fail with, phrased so its `Display` names
/// what went wrong and what to do. `main` turns it into one line on
/// stderr and a failing exit code — the only place the CLI writes an
/// error.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// A filesystem or process step the command needed failed; `context`
    /// says what it was doing so the message names the fix.
    #[error("failed to {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// The current directory could not be read — every command that
    /// resolves a project needs it, so it fails before doing anything.
    #[error("cannot determine the current directory: {source}")]
    Cwd {
        #[source]
        source: std::io::Error,
    },

    /// A run this binary has no record of, wherever it looked. One
    /// sentence, because a person who mistyped an id gets the same
    /// answer whichever command they typed it into — with the run it
    /// was one slip away from, when there is one.
    #[error("no run is called `{id}`{}", .near.as_ref().map(|near| format!(" — did you mean `{near}`?")).unwrap_or_default())]
    RunNotFound {
        id: RunId,
        /// Where it looked, as a person reads a path.
        roots: Vec<String>,
        near: Option<String>,
    },

    #[error(transparent)]
    Project(#[from] crate::project::ProjectError),

    #[error(transparent)]
    Pack(#[from] crate::pack::PackError),

    #[error(transparent)]
    Storage(#[from] yunta_storage::StorageError),

    #[error(transparent)]
    Catalog(#[from] yunta_engine::CatalogError),

    #[error(transparent)]
    Run(#[from] yunta_engine::RunError),

    #[error(transparent)]
    ManifestRead(#[from] yunta_engine::ManifestReadError),

    #[error(transparent)]
    Manifest(#[from] yunta_engine::ManifestError),

    #[error(transparent)]
    DetachedResume(#[from] crate::commands::DetachedResumeError),

    #[error(transparent)]
    ResolveGate(#[from] yunta_engine::ResolveGateError),

    /// A decision was put to a run that is not parked at one. The
    /// engine's sentence says what is true of the run; which command
    /// shows a reader where it actually is is this border's word, so it
    /// is added here — named by its handle for a person, and by its whole
    /// id for an agent ([`CliError::said_to`]).
    #[error("{refusal} — `{}` shows where it is", crate::commands::advice::status(.run_id.handle()))]
    NotPaused {
        run_id: RunId,
        #[source]
        refusal: yunta_engine::ResolveGateError,
    },

    /// A decision was put to a run whose pause reconstructs no menu.
    /// Those pauses are settled where they were raised — a budget, a
    /// scope, an answers file, a review on a forge — and the run handed
    /// back, which is what the advice names.
    #[error("{refusal} — settle it where it was raised, then `{}`", crate::commands::advice::resume(.run_id.handle()))]
    NoMenu {
        run_id: RunId,
        #[source]
        refusal: yunta_engine::ResolveGateError,
    },

    /// A mock fixture a `yunta test` case or `yunta run --fixture`
    /// named does not parse. The path leads the sentence, because a
    /// person running several cases needs to know which fixture broke
    /// before they need to know how.
    #[error("fixture `{}`: {source}", .path.display())]
    FixtureRefused {
        path: std::path::PathBuf,
        #[source]
        source: yunta_adapters::FixtureError,
    },

    /// A decision was recorded and the run could not be handed back to
    /// a detached `yunta resume`. The sentence leads with what did
    /// happen, because a reader who takes this for a refusal answers
    /// the same gate twice.
    #[error("decision recorded, but {source}")]
    GateRecordedNotResumed {
        #[source]
        source: crate::commands::DetachedResumeError,
    },

    /// A run that cannot be closed. The engine's sentence says what is
    /// true of the run; which command moves it instead is this border's
    /// word.
    #[error("{refusal}{}", close_advice(.run_id.handle(), .refusal))]
    CloseRefused {
        run_id: RunId,
        #[source]
        refusal: yunta_engine::CloseRunError,
    },

    /// A run named by something more than one run answers to. Every
    /// run it could be is listed whole, so the next command can name one.
    #[error("{said} — name one:{}", one_per_line(.candidates))]
    AmbiguousRun {
        said: String,
        candidates: Vec<crate::commands::run_ref::Candidate>,
    },

    /// A document that does not read, or breaks its own rules: every
    /// problem it has, each quoted from the text it was read from.
    #[error("{}", crate::render::blocks::diagnostic::report(.report, .text.as_deref()))]
    Document {
        report: yunta_core::diagnostic::Report,
        text: Option<String>,
    },

    /// A document kind this binary does not publish. The sentence
    /// lists the kinds that exist, so `yunta schema` and the
    /// `document_shape` tool answer the same mistake the same way.
    #[error(transparent)]
    UnknownArtifactKind(#[from] yunta_core::UnknownArtifactKind),

    /// A reply that does not answer the questions it claims to, or a
    /// round that could not be read back — the engine's own verdict,
    /// which is the same one a person at a console gets.
    #[error(transparent)]
    AnswerQuestions(#[from] yunta_engine::AnswerQuestionsError),

    /// The answers were recorded and the run could not be handed back
    /// to a detached `yunta resume`. They are on the log either way,
    /// which is what the sentence leads with: a reader who takes this
    /// for a refusal answers the same questions twice.
    #[error("answers recorded, but {source}")]
    AnswersRecordedNotResumed {
        #[source]
        source: crate::commands::DetachedResumeError,
    },

    /// A value that has to be an identifier and is not — a run id, an
    /// adapter, a mode, a gate option, a responder — wherever one is
    /// read off an argument.
    #[error(transparent)]
    InvalidId(#[from] yunta_core::InvalidId),

    #[error(transparent)]
    Worktree(#[from] yunta_engine::WorktreeError),

    #[error(transparent)]
    FrozenPaths(#[from] yunta_core::RelativeRootError),

    /// A condition specific to one command, already phrased as an
    /// actionable message at the point it is detected — the CLI's own
    /// border for something no shared type names.
    ///
    /// Exceptional, and meant to stay that way: a failure two commands
    /// can reach, or one a caller has to tell apart from another, earns
    /// an arm of its own. A `String` here is a failure that has exactly
    /// one site and nothing to match on.
    #[error("{0}")]
    Message(String),
}

impl CliError {
    /// Builds an [`CliError::Io`] whose `context` is `verb`ing `subject`
    /// (e.g. `io("write", path.display())`).
    pub fn io(verb: &str, subject: impl Display, source: std::io::Error) -> Self {
        CliError::Io {
            context: format!("{verb} {subject}"),
            source,
        }
    }

    /// A one-off actionable message no shared error type names.
    pub fn msg(message: impl Into<String>) -> Self {
        CliError::Message(message.into())
    }

    /// An engine refusal to record a decision, in this border's
    /// vocabulary.
    ///
    /// Two of them describe the state the run is in rather than
    /// anything about the request, and a reader told the run cannot be
    /// answered wants to know what to do instead. Which command does
    /// that is the CLI's word, not the engine's, so it is said here —
    /// and every other refusal already names what to change (an option
    /// off the menu lists the ones that are on it) and passes through
    /// untouched.
    pub fn gate_refused(run_id: &RunId, refusal: yunta_engine::ResolveGateError) -> Self {
        let run_id = run_id.clone();
        match refusal {
            yunta_engine::ResolveGateError::NotPaused => CliError::NotPaused { run_id, refusal },
            yunta_engine::ResolveGateError::NothingToResolve => {
                CliError::NoMenu { run_id, refusal }
            }
            yunta_engine::ResolveGateError::UnknownOption { .. }
            | yunta_engine::ResolveGateError::Unsaid { .. }
            | yunta_engine::ResolveGateError::Storage(_)
            | yunta_engine::ResolveGateError::Documents(_) => refusal.into(),
        }
    }
}

/// Every run a reference could mean, one to a line under the sentence:
/// the id whole, and what tells it apart when there is something to say.
fn one_per_line(candidates: &[crate::commands::run_ref::Candidate]) -> String {
    candidates
        .iter()
        .map(|candidate| match &candidate.about {
            Some(about) => format!("\n  {}  {about}", candidate.run_id),
            None => format!("\n  {}", candidate.run_id),
        })
        .collect()
}

/// What to do instead of closing a run the engine would not close,
/// naming the run as `run`.
fn close_advice(run: &str, refusal: &yunta_engine::CloseRunError) -> String {
    match refusal {
        yunta_engine::CloseRunError::Driven => format!(
            " — `{}` stops it, and then it can be closed",
            crate::commands::advice::cancel(run)
        ),
        yunta_engine::CloseRunError::Moving => format!(
            " — `{}` shows where it is",
            crate::commands::advice::status(run)
        ),
        yunta_engine::CloseRunError::AlreadyClosed
        | yunta_engine::CloseRunError::Storage(_)
        | yunta_engine::CloseRunError::Export(_) => String::new(),
    }
}

/// Who a refusal is said to: a person, who types the next command and
/// reads a run's handle; or an agent, which copies a run's whole id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    Person,
    Agent,
}

impl CliError {
    /// The refusal as `reader` reads it, with what caused it: a run's
    /// advice names it by its handle for a person, by its whole id for
    /// an agent.
    pub fn said_to(&self, reader: Reader) -> String {
        let id = |run_id: &RunId| match reader {
            Reader::Person => run_id.handle().to_string(),
            Reader::Agent => run_id.to_string(),
        };
        let said = match self {
            CliError::NotPaused { run_id, refusal } => format!(
                "{refusal} — `{}` shows where it is",
                crate::commands::advice::status(&id(run_id))
            ),
            CliError::NoMenu { run_id, refusal } => format!(
                "{refusal} — settle it where it was raised, then `{}`",
                crate::commands::advice::resume(&id(run_id))
            ),
            CliError::CloseRefused { run_id, refusal } => {
                format!("{refusal}{}", close_advice(&id(run_id), refusal))
            }
            other => other.to_string(),
        };
        // What caused it goes with it, to a person and an agent alike: a
        // refusal that stops at its own sentence hides why.
        yunta_core::with_causes(said, self)
    }

    /// What a person is told beside the refusal: where it looked, and the
    /// commands that go on from here. Empty for a refusal that says it
    /// all in its sentence.
    pub fn advice(&self) -> (Option<String>, Vec<(String, &'static str)>) {
        match self {
            CliError::RunNotFound { roots, .. } => (
                Some(format!("looked in {}", roots.join(" and "))),
                vec![
                    (
                        "yunta list --runs".to_string(),
                        "this repository's runs, what needs you first",
                    ),
                    (
                        "yunta list --runs --all".to_string(),
                        "every run on this machine",
                    ),
                ],
            ),
            _ => (None, Vec::new()),
        }
    }

    /// An engine refusal to close a run, in this border's vocabulary.
    pub fn close_refused(run_id: &RunId, refusal: yunta_engine::CloseRunError) -> Self {
        CliError::CloseRefused {
            run_id: run_id.clone(),
            refusal,
        }
    }
}

/// A warning to stderr — the one place the CLI prints `warning:` lines,
/// so a run that succeeds with caveats still says so without an error.
/// The word is painted in the caution color on a stream that draws one,
/// so a warning said before a run starts is not lost among the lines
/// around it.
pub fn warn(message: impl Display) {
    eprintln!("{}: {message}", warning_word());
}

/// `warning`, as stderr is painted.
pub(crate) fn warning_word() -> String {
    crate::render::stderr_ink().word(crate::render::ink::Tone::Caution, "warning")
}

/// An informational block to stderr — verification findings and the
/// like, printed beside a command's own output without claiming to be an
/// error.
pub fn note(message: impl Display) {
    eprintln!("{message}");
}
