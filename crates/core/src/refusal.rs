//! What Yunta refuses to do to a run, as data: a decision it will not
//! record, a reply to its questions it will not take, a close it will not
//! make. The engine returns it; render says it, with the commands that go
//! on from it.
//!
//! Not a failure: a refusal is what is true of the run or of the request, so
//! a person reads it and does something else. A failure is something below
//! that broke (the store, a document), said in its own words with its causes.

use crate::diagnostic::Report;
use crate::events::{Asks, Withheld};
use crate::{NodeId, OptionId};

/// Why a decision put to a run is not recorded. Each variant's display is
/// its code; the words a person reads are render's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GateRefusal {
    /// The run is not stopped at a pause: a live process may drive it,
    /// or it already finished.
    #[error("not_paused")]
    NotPaused,
    /// The run is paused for a reason with no menu of options to answer.
    #[error("nothing_to_resolve")]
    NothingToResolve,
    /// The chosen option is not on the menu; `declared` lists the options it holds.
    #[error("unknown_option")]
    UnknownOption { chosen: OptionId, declared: String },
    /// The option sends the run back to a session, and the words it asks for
    /// are the one thing that session would get.
    #[error("unsaid")]
    Unsaid { chosen: OptionId, asks: Asks },
    /// The gate does not offer the option, and `withheld` says why.
    #[error("withheld")]
    Withheld {
        chosen: OptionId,
        withheld: Withheld,
    },
}

/// Why a run is not closed. Each variant's display is its code; the words a
/// person reads are render's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CloseRefusal {
    /// Its log already says it closed.
    #[error("already_closed")]
    AlreadyClosed,
    /// An engine that lives drives it.
    #[error("driven")]
    Driven,
    /// Its log says it is moving and nothing says the engine that drove
    /// it is gone: it may be between two processes.
    #[error("moving")]
    Moving,
}

/// Why a reply to a node's questions is not recorded. Each variant's display
/// is its code; the words a person reads are render's.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AnswersRefusal {
    /// The node has no open round. Either it never asked, or its questions
    /// are already answered, and a second reply to a closed round is not one.
    #[error("nothing_asked")]
    NothingAsked { node: NodeId },
    /// The reply does not answer the questions it claims to, every way it
    /// fails named. Nothing was written: a surface corrects and replies again.
    #[error("rejected")]
    Rejected(Report),
}
