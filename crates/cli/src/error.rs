//! The one error a command hands back and the one place its text reaches
//! a person. Every subcommand returns a `Result` of an
//! [`Outcome`](crate::output::Outcome) or a [`CliError`]; `main` prints a
//! [`CliError`] once, on stderr, and maps the exit code. A command that ran
//! but reports a failing verdict (an invalid workflow, a test case that
//! failed) returns [`Outcome::Reported`](crate::output::Outcome::Reported)
//! instead — its detail is already on the command's own output, and no error
//! banner belongs on top of it.

use std::fmt::Display;
use std::path::PathBuf;

use surco::words::RUNS_THAT_NEED_YOU_FIRST;
use yunta_core::step::{NextStep, Reach};
use yunta_core::{Pid, RunId};
use yunta_render::command::{self, Reader};

pub use crate::output::{note, warn};

mod hold;
pub(crate) mod page;
pub(crate) mod words;

use page::{document_problems, unrunnable_said};

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
    #[error("no run is called `{id}`{}", surco::text::near_miss(.near.as_deref()))]
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

    /// A git call the command made that did not answer. A stop the token
    /// made is the command's cancellation, not a failed git: see the
    /// conversion below.
    #[error(transparent)]
    Git(yunta_engine::GitError),

    /// A manifest that could not be frozen. A stop the token made while git
    /// answered it is the command's cancellation, not a failed manifest: see
    /// the conversion below.
    #[error(transparent)]
    Manifest(yunta_engine::ManifestError),

    #[error(transparent)]
    DetachedResume(#[from] crate::commands::DetachedResumeError),

    #[error(transparent)]
    ResolveGate(#[from] yunta_engine::ResolveGateError),

    /// A decision was put to a run that is not parked at one. The words of
    /// the refusal say what is true of the run; which command shows a
    /// reader where it actually is is this border's word, so it
    /// is added here — named by its handle for a person, and by its whole
    /// id for an agent ([`CliError::said_to`]).
    #[error("{}; `{}` shows where it is", words::resolve_gate(.refusal), command::line(&NextStep::status(.run_id)))]
    NotPaused {
        run_id: RunId,
        refusal: yunta_engine::ResolveGateError,
    },

    /// A decision was put to a run whose pause reconstructs no menu.
    /// Those pauses are settled where they were raised — a scope, an
    /// answers file, a review on a forge, a child — and the run handed
    /// back, which is what the advice names.
    #[error("{}; settle it where it was raised, then `{}`", words::resolve_gate(.refusal), command::line(&NextStep::Resume { run: RunId::clone(.run_id) }))]
    NoMenu {
        run_id: RunId,
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

    /// A run that cannot be closed. The words of the refusal say what is
    /// true of the run; which command moves it instead is this border's
    /// word.
    #[error("{}{}", words::close(.refusal), close_advice(.run_id, Reader::Person, .refusal))]
    CloseRefused {
        run_id: RunId,
        refusal: yunta_engine::CloseRunError,
    },

    /// A run another engine drives. The sentence says whose it is;
    /// [`CliError::advice`] the commands that follow it and stop it.
    #[error("{}", words::moving(.run_id.handle(), *.pid))]
    RunMoving { run_id: RunId, pid: Pid },

    /// A run whose engine is asking a person at its terminal, the one place
    /// that moves it.
    #[error("{}", yunta_render::engine::asking_claim(.asking))]
    RunAsking {
        asking: yunta_replay::registry::Asking,
    },

    /// A claim whose holder cannot be told from a leftover: a pid this user
    /// cannot signal, or a file that names nobody. Taking it could put two engines
    /// on one run, so the sentence names the file for a person to decide.
    #[error("{}", words::hold_unclear(.run_id.handle(), .lock_path, *.pid))]
    RunHoldUnclear {
        run_id: RunId,
        lock_path: PathBuf,
        pid: Option<Pid>,
    },

    /// A run named by something more than one run answers to. Every
    /// run it could be is listed whole, so the next command can name one.
    #[error("{}:{}", page::name_one(.said), one_per_line(.candidates))]
    AmbiguousRun {
        said: String,
        candidates: Vec<crate::commands::run_ref::Candidate>,
    },

    /// A document that does not read, or breaks its own rules: every
    /// problem it has, each quoted from the text it was read from.
    #[error("{}", yunta_render::blocks::diagnostic::plain(&.report.document.path, document_problems(.report, .text.as_deref())))]
    Document {
        report: yunta_core::diagnostic::Report,
        text: Option<String>,
    },

    /// A workflow that cannot run here: every problem `yunta check` finds
    /// in it, each pointed at in its line, and what this project could
    /// declare for the ones a declaration fixes.
    #[error("{}", unrunnable_said(.0))]
    Unrunnable(crate::commands::verdict::Unrunnable),

    /// A document this binary does not show. The sentence lists the ones
    /// it does, and the one a slip most likely meant.
    #[error(transparent)]
    UnknownDocument(#[from] yunta_core::UnknownDocument),

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
            | yunta_engine::ResolveGateError::Withheld { .. }
            | yunta_engine::ResolveGateError::Storage(_)
            | yunta_engine::ResolveGateError::Documents(_) => refusal.into(),
        }
    }
}

/// A git the token stopped is the command's cancellation, the one a run's
/// own git reaches; any other git failure is a git failure.
impl From<yunta_engine::GitError> for CliError {
    fn from(error: yunta_engine::GitError) -> Self {
        match error.cancelled() {
            true => CliError::Run(yunta_engine::RunError::Cancelled),
            false => CliError::Git(error),
        }
    }
}

/// A manifest the token stopped while git answered it is the run's
/// cancellation, the same one a run's own git reaches; any other manifest
/// failure stays a manifest failure.
impl From<yunta_engine::ManifestError> for CliError {
    fn from(error: yunta_engine::ManifestError) -> Self {
        match error.cancelled() {
            true => CliError::Run(yunta_engine::RunError::Cancelled),
            false => CliError::Manifest(error),
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

/// What to do instead of closing a run the engine would not close, the
/// run named the way `reader` copies it.
fn close_advice(run: &RunId, reader: Reader, refusal: &yunta_engine::CloseRunError) -> String {
    match refusal {
        yunta_engine::CloseRunError::Driven => format!(
            "; `{}` stops it, and then it can be closed",
            command::line_for(&NextStep::Cancel { run: run.clone() }, reader)
        ),
        yunta_engine::CloseRunError::Moving => format!(
            "; `{}` shows where it is",
            command::line_for(&NextStep::status(run), reader)
        ),
        yunta_engine::CloseRunError::AlreadyClosed
        | yunta_engine::CloseRunError::Storage(_)
        | yunta_engine::CloseRunError::Export(_) => String::new(),
    }
}

impl CliError {
    /// The words of a refusal whose sentence already names its cause, which
    /// the chain of causes would otherwise append a second time.
    fn carried_words(&self) -> Option<String> {
        match self {
            CliError::ResolveGate(refusal @ yunta_engine::ResolveGateError::Documents(_)) => {
                Some(words::resolve_gate(refusal))
            }
            CliError::Run(yunta_engine::RunError::Artifact(
                yunta_engine::AcceptError::Project { name, source },
            )) => Some(words::project_view(name, source)),
            _ => None,
        }
    }

    /// The refusal as `reader` reads it, with what caused it: a run's
    /// advice names it by its handle for a person, by its whole id for
    /// an agent. A refusal that found problems in a text says how many to
    /// a person, whose page points at each in its line, and says each to
    /// an agent, which reads no page.
    pub fn said_to(&self, reader: Reader) -> String {
        if let Some(said) = self.carried_words() {
            return said;
        }
        if let Some(headline) = self
            .headline_for_a_person()
            .filter(|_| reader == Reader::Person)
        {
            return headline;
        }
        let said = self.sentence(reader);
        // What caused it goes with it, to a person and an agent alike: a
        // refusal that stops at its own sentence hides why.
        yunta_core::with_causes(said, self)
    }

    /// The refusal's own sentence, any run it names named the way `reader`
    /// copies it: the words of a refusal its type says, and its display for
    /// any other.
    fn sentence(&self, reader: Reader) -> String {
        let id = |run_id: &RunId| reader.name_of(run_id).to_string();
        match self {
            CliError::ResolveGate(refusal) => words::resolve_gate(refusal),
            CliError::NotPaused { run_id, refusal } => format!(
                "{}; `{}` shows where it is",
                words::resolve_gate(refusal),
                command::line_for(&NextStep::status(run_id), reader)
            ),
            CliError::NoMenu { run_id, refusal } => format!(
                "{}; settle it where it was raised, then `{}`",
                words::resolve_gate(refusal),
                command::line_for(
                    &NextStep::Resume {
                        run: run_id.clone()
                    },
                    reader
                )
            ),
            CliError::CloseRefused { run_id, refusal } => format!(
                "{}{}",
                words::close(refusal),
                close_advice(run_id, reader, refusal)
            ),
            CliError::RunMoving { run_id, pid } => words::moving(&id(run_id), *pid),
            CliError::RunHoldUnclear {
                run_id,
                lock_path,
                pid,
            } => words::hold_unclear(&id(run_id), lock_path, *pid),
            CliError::Manifest(yunta_engine::ManifestError::Inputs(inputs)) => {
                words::inputs(inputs)
            }
            CliError::Run(yunta_engine::RunError::Object(problem)) => {
                yunta_render::failure::object(problem)
            }
            CliError::Run(yunta_engine::RunError::RefusedAnswer {
                refused,
                summary,
                failure,
            }) => words::refused_answer(refused, summary, failure.as_deref()),
            other => other.to_string(),
        }
    }

    /// What `reader` is told beside the refusal: where it looked, and the
    /// commands that go on from here. Empty for a refusal that says it
    /// all in its sentence.
    pub fn advice(&self, reader: Reader) -> (Option<String>, Vec<(String, &'static str)>) {
        match self {
            CliError::RunMoving { run_id, .. } => (
                None,
                vec![
                    (
                        command::line_for(
                            &NextStep::Follow {
                                run: run_id.clone(),
                            },
                            reader,
                        ),
                        "follows it",
                    ),
                    (
                        command::line_for(
                            &NextStep::Cancel {
                                run: run_id.clone(),
                            },
                            reader,
                        ),
                        "stops it",
                    ),
                ],
            ),
            CliError::RunNotFound { roots, .. } => (
                Some(format!("looked in {}", roots.join(" and "))),
                vec![
                    (
                        command::line(&NextStep::ListRuns {
                            reach: Reach::Repository,
                        }),
                        RUNS_THAT_NEED_YOU_FIRST,
                    ),
                    (
                        command::line(&NextStep::ListRuns {
                            reach: Reach::Machine,
                        }),
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

/// `warning`, as stderr is painted.
pub(crate) fn warning_word() -> String {
    crate::output::stderr_paint().word(surco::line::Tone::Caution, "warning")
}
