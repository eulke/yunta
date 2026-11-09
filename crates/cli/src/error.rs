//! The one error a command hands back and the one place its text reaches
//! a person. Every subcommand returns a `Result` of an
//! [`Outcome`](crate::output::Outcome) or a [`CliError`]; `main` prints a
//! [`CliError`] once, on stderr, and maps the exit code. A command that ran
//! but reports a failing verdict (an invalid workflow, a test case that
//! failed) returns [`Outcome::Reported`](crate::output::Outcome::Reported)
//! instead — its detail is already on the command's own output, and no error
//! banner belongs on top of it.

use std::fmt::Display;

use yunta_core::RunId;
use yunta_render::command::Reader;
use yunta_render::refusal::Refused;

mod hold;
pub(crate) mod page;
pub(crate) mod words;

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

    #[error(transparent)]
    Project(#[from] crate::project::ProjectError),

    #[error(transparent)]
    Pack(#[from] crate::pack::PackError),

    #[error(transparent)]
    Storage(#[from] yunta_storage::StorageError),

    #[error(transparent)]
    Catalog(#[from] yunta_engine::CatalogError),

    /// What the run failed with. The refusals it carries (an object its
    /// store lacks, an answer no decision takes) reach a command as
    /// [`CliError::Refused`]: see the conversion below.
    #[error(transparent)]
    Run(yunta_engine::RunError),

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

    /// What recording a decision failed with below its refusal: the store,
    /// or the documents the gate shows. A refusal is
    /// [`CliError::Refused`], through [`CliError::from_gate`].
    #[error(transparent)]
    ResolveGate(yunta_engine::ResolveGateError),

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

    /// What closing a run failed with below its refusal: the store, or the
    /// run's exported log. A refusal is [`CliError::Refused`], through
    /// [`CliError::from_close`].
    #[error("{}", words::close(.0))]
    Close(yunta_engine::CloseRunError),

    /// What recording a reply to a node's questions failed with below its
    /// refusal. A refusal is [`CliError::Refused`]: see the conversion below.
    #[error(transparent)]
    AnswerQuestions(yunta_engine::AnswerQuestionsError),

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

    /// What Yunta refused: render says it, to a person and to an agent,
    /// with the commands that go on from it.
    #[error(transparent)]
    Refused(#[from] Refused),

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

    /// What recording a decision on `run_id` met: the run's refusal, which
    /// names the run for the command that goes on, or a failure below it.
    pub fn from_gate(run_id: &RunId, error: yunta_engine::ResolveGateError) -> Self {
        match error {
            yunta_engine::ResolveGateError::Refused(refusal) => Refused::Gate {
                run: run_id.clone(),
                refusal,
            }
            .into(),
            failed => CliError::ResolveGate(failed),
        }
    }

    /// What closing `run_id` met: the run's refusal, which names the run for
    /// the command that moves it instead, or a failure below it.
    pub fn from_close(run_id: &RunId, error: yunta_engine::CloseRunError) -> Self {
        match error {
            yunta_engine::CloseRunError::Refused(refusal) => Refused::Close {
                run: run_id.clone(),
                refusal,
            }
            .into(),
            failed => CliError::Close(failed),
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
/// cancellation, the same one a run's own git reaches; inputs it does not take
/// are refused; any other manifest failure stays a manifest failure.
impl From<yunta_engine::ManifestError> for CliError {
    fn from(error: yunta_engine::ManifestError) -> Self {
        match error {
            yunta_engine::ManifestError::Inputs(problem) => Refused::Inputs(problem).into(),
            error if error.cancelled() => CliError::Run(yunta_engine::RunError::Cancelled),
            error => CliError::Manifest(error),
        }
    }
}

/// What a run met: an object its store lacks and an answer no decision takes
/// are refusals; anything else it failed with is the run's failure.
impl From<yunta_engine::RunError> for CliError {
    fn from(error: yunta_engine::RunError) -> Self {
        match error {
            yunta_engine::RunError::Object(problem) => Refused::Object(problem).into(),
            yunta_engine::RunError::RefusedAnswer {
                refused,
                summary,
                failure,
            } => Refused::SurfaceAnswer {
                refused,
                claim: summary,
                menu: failure,
            }
            .into(),
            failed => CliError::Run(failed),
        }
    }
}

/// A reply the run does not take is refused; anything else recording it met
/// failed below.
impl From<yunta_engine::AnswerQuestionsError> for CliError {
    fn from(error: yunta_engine::AnswerQuestionsError) -> Self {
        match error {
            yunta_engine::AnswerQuestionsError::Refused(refusal) => {
                Refused::Answers(refusal).into()
            }
            failed => CliError::AnswerQuestions(failed),
        }
    }
}

/// A document this binary does not show is refused, naming the ones it does.
impl From<yunta_core::UnknownDocument> for CliError {
    fn from(unknown: yunta_core::UnknownDocument) -> Self {
        Refused::UnknownDocument(unknown).into()
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
        let said = match self {
            CliError::Refused(refused) => refused.said(reader),
            CliError::ResolveGate(error) => words::resolve_gate(error),
            failed => failed.to_string(),
        };
        // What caused it goes with it, to a person and an agent alike: a
        // refusal that stops at its own sentence hides why.
        yunta_core::with_causes(said, self)
    }

    /// What `reader` is told beside the refusal: where it looked, and the
    /// commands that go on from here. Empty for a refusal that says it
    /// all in its sentence.
    pub fn advice(&self, reader: Reader) -> (Option<String>, Vec<(String, &'static str)>) {
        match self {
            CliError::Refused(refused) => refused.advice(reader),
            _ => (None, Vec::new()),
        }
    }
}

/// `warning`, as stderr is painted.
pub(crate) fn warning_word() -> String {
    crate::output::stderr_paint().word(surco::line::Tone::Caution, "warning")
}
