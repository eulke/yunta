//! What a findings document's rules found wrong with it, and what the run
//! found wrong with a finding or a withdrawal a session handed over, as the
//! values each clause names. The clause is what a reader acts on; the code is
//! the stable name a receipt counts the rule by.

use serde::{Deserialize, Serialize};

use crate::diagnostic::RuleCode;

/// What a rule of a findings document, or of a finding a node handed over,
/// found wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FindingFault {
    DuplicateFinding,
    EmptyTitle,
    EmptyDetail,
    EmptyReason,
    PostedTwice,
    WithdrawnId,
    NeverPosted,
}

impl FindingFault {
    /// The stable name of the rule this fault breaks.
    pub fn code(&self) -> RuleCode {
        match self {
            Self::DuplicateFinding | Self::PostedTwice => RuleCode::DuplicateId,
            Self::EmptyTitle => RuleCode::EmptyTitle,
            Self::EmptyDetail => RuleCode::EmptyDetail,
            Self::EmptyReason => RuleCode::EmptyReason,
            Self::WithdrawnId => RuleCode::WithdrawnId,
            Self::NeverPosted => RuleCode::UnknownId,
        }
    }

    /// The clause a reader acts on, in the words the rule has always been
    /// written in.
    pub fn clause(&self) -> String {
        match self {
            Self::DuplicateFinding => {
                "a second finding already carries this id; every id is declared once".to_string()
            }
            Self::EmptyTitle => "`title` is empty".to_string(),
            Self::EmptyDetail => "`detail` is empty".to_string(),
            Self::EmptyReason => {
                "say why it no longer stands, so the log keeps the reason".to_string()
            }
            Self::PostedTwice => {
                "this node already reported a finding under that id; report the new one under an \
                 id of its own, or update the one that stands"
                    .to_string()
            }
            Self::WithdrawnId => {
                "this node withdrew that id, and a withdrawal is final; a finding that comes back \
                 is a new id"
                    .to_string()
            }
            Self::NeverPosted => {
                "this node never reported a finding under that id; only the node that reported \
                 one can change it"
                    .to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FindingFault;

    #[test]
    fn each_clause_reads_as_the_findings_have_always_said_it() {
        assert_eq!(FindingFault::EmptyTitle.clause(), "`title` is empty");
        assert_eq!(FindingFault::EmptyDetail.clause(), "`detail` is empty");
        assert_eq!(
            FindingFault::PostedTwice.clause(),
            "this node already reported a finding under that id; report the new one under an id \
             of its own, or update the one that stands"
        );
        assert_eq!(
            FindingFault::WithdrawnId.clause(),
            "this node withdrew that id, and a withdrawal is final; a finding that comes back is a \
             new id"
        );
        assert_eq!(
            FindingFault::NeverPosted.clause(),
            "this node never reported a finding under that id; only the node that reported one can \
             change it"
        );
        assert_eq!(
            FindingFault::PostedTwice.code().as_str(),
            "duplicate-id",
            "a second report under one id is a duplicate of the first"
        );
    }
}
