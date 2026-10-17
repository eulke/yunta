//! What a questions document's rules found wrong with it, and what a reply
//! found wrong with the questions it answers, as the values each clause
//! names. The clause is what a reader acts on; the code is the stable name a
//! receipt counts the rule by.

use serde::{Deserialize, Serialize};

use crate::diagnostic::RuleCode;

/// What a rule of a questions document, or of the answers to one, found
/// wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum QuestionFault {
    DuplicateQuestion,
    EmptyText,
    MissingValues,
    UnstatedAssumption,
    DuplicateAnswer,
    MissingAnswer,
    UnknownQuestion,
    NotAValue { value: String, allowed: Vec<String> },
    NotABoolean { value: String },
}

impl QuestionFault {
    /// The stable name of the rule this fault breaks.
    pub fn code(&self) -> RuleCode {
        match self {
            Self::DuplicateQuestion | Self::DuplicateAnswer => RuleCode::DuplicateId,
            Self::EmptyText => RuleCode::EmptyText,
            Self::MissingValues => RuleCode::MissingValues,
            Self::UnstatedAssumption => RuleCode::UnstatedAssumption,
            Self::MissingAnswer => RuleCode::MissingAnswer,
            Self::UnknownQuestion => RuleCode::UnknownId,
            Self::NotAValue { .. } | Self::NotABoolean { .. } => RuleCode::MismatchedAnswer,
        }
    }

    /// The clause a reader acts on, in the words the rule has always been
    /// written in.
    pub fn clause(&self) -> String {
        match self {
            Self::DuplicateQuestion => {
                "a second question already carries this id; every id is declared once".to_string()
            }
            Self::EmptyText => {
                "`text` is empty; a person has to be able to read the question".to_string()
            }
            Self::MissingValues => {
                "`answer_type` is `choice` but `values` is empty; list the answers allowed"
                    .to_string()
            }
            Self::UnstatedAssumption => "the question is optional but says nothing in `assumes`; \
                 say what the work takes as the answer when nobody gives one"
                .to_string(),
            Self::DuplicateAnswer => {
                "this question is already answered above; one question has one answer".to_string()
            }
            Self::MissingAnswer => "this question is `required` and nothing answers it".to_string(),
            Self::UnknownQuestion => {
                "no question carries this id; an answer answers a question that was asked"
                    .to_string()
            }
            Self::NotAValue { value, allowed } => format!(
                "`{value}` is not one of the values this question allows: {}",
                allowed.join(", ")
            ),
            Self::NotABoolean { value } => format!("`{value}` is not `true` or `false`"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QuestionFault;

    #[test]
    fn each_clause_reads_as_the_questions_have_always_said_it() {
        assert_eq!(
            QuestionFault::UnstatedAssumption.clause(),
            "the question is optional but says nothing in `assumes`; say what the work takes as \
             the answer when nobody gives one"
        );
        assert_eq!(
            QuestionFault::MissingValues.clause(),
            "`answer_type` is `choice` but `values` is empty; list the answers allowed"
        );
        assert_eq!(
            QuestionFault::NotAValue {
                value: "maybe".to_string(),
                allowed: vec!["yes".to_string(), "no".to_string()],
            }
            .clause(),
            "`maybe` is not one of the values this question allows: yes, no"
        );
        assert_eq!(
            QuestionFault::NotABoolean {
                value: "1".to_string(),
            }
            .clause(),
            "`1` is not `true` or `false`"
        );
        assert_eq!(
            QuestionFault::UnknownQuestion.code().as_str(),
            "unknown-id",
            "an answer to a question nobody asked is an unknown id"
        );
    }
}
