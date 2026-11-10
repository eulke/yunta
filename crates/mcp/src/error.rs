//! See [`crate`]. Why a tool did not do what it was asked, and what the agent
//! that called it is told: the sentence the equivalent command prints on
//! stderr, with what caused it.

use yunta_render::command::Reader;
use yunta_render::refusal::Refused;

/// Why a tool did not do what it was asked: an argument it could not read,
/// or what the API refused or failed with.
#[derive(Debug)]
pub(crate) enum ToolError {
    /// An argument missing, of another type, or not what it has to be: what
    /// is wrong with it.
    Argument(String),
    /// What the API did not do.
    Api(yunta_api::Error),
    /// An answer the tool could not write out: what writing it said.
    Unwritten(String),
}

impl ToolError {
    /// What the agent that called the tool is told: a refusal as an agent
    /// reads it, a run named by its whole id; anything else in its own words
    /// and its causes'.
    pub(crate) fn said(&self) -> String {
        match self {
            ToolError::Argument(said) | ToolError::Unwritten(said) => said.clone(),
            ToolError::Api(yunta_api::Error::Refused(refused)) => {
                yunta_core::with_causes(refused.said(Reader::Agent), refused)
            }
            ToolError::Api(error) => yunta_core::describe(error),
        }
    }

    /// A value that has to be an identifier and is not, said as it is.
    pub(crate) fn invalid(id: yunta_core::InvalidId) -> Self {
        ToolError::Argument(yunta_core::describe(&id))
    }

    /// A document this binary does not show, refused naming the ones it
    /// does.
    pub(crate) fn unknown(unknown: yunta_core::UnknownDocument) -> Self {
        ToolError::Api(Refused::UnknownDocument(unknown).into())
    }
}

/// What the API did not do, as a tool says it.
impl From<yunta_api::Error> for ToolError {
    fn from(error: yunta_api::Error) -> Self {
        ToolError::Api(error)
    }
}
