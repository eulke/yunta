//! The escalation options the engine itself builds and interprets. Their
//! spellings live here once, so no run-loop branch ever matches a human's
//! choice or a node's outcome against a bare string literal — the reserved
//! vocabulary is one closed enum, built from it and read back through it.

use std::str::FromStr;

use crate::ids::OptionId;

/// An option the engine appends to an escalation and acts on when a human
/// picks it — as opposed to an author-declared option, which the engine
/// only records. Everything the run loop decides on goes through this
/// enum, never a literal. A log stores it as its spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ReservedOption {
    /// Pause the run with the decision recorded (every escalation offers it).
    Abort,
    /// Run a failed node again: through its corrective node once more,
    /// past the reroute cap, or — for a node with no re-route — itself.
    Retry,
    /// Run a failed loop again from the work its blocked tasks left,
    /// judging that work before any session opens.
    ContinueWork,
    /// Accept promotion to the next declared mode.
    Promote,
    /// Approve a published gate.
    Approve,
    /// Grant a scope expansion a task requested, or widen a failed
    /// node's scope by the paths its failure needs and run it again.
    Grant,
    /// Deny a scope expansion a task requested.
    Deny,
    /// Lift a budget cap for this invocation only.
    Continue,
    /// Reject a published gate resolved from the console.
    Reject,
    /// Accept a departure from the plan a task's session declared: the
    /// task closes on the work as it stands.
    Accept,
    /// Send a departure back to the session that made it, with what the
    /// person says it should do instead.
    SendBack,
}

impl ReservedOption {
    /// Every reserved option, in declaration order.
    pub fn all() -> Vec<Self> {
        vec![
            Self::Abort,
            Self::Retry,
            Self::ContinueWork,
            Self::Promote,
            Self::Approve,
            Self::Grant,
            Self::Deny,
            Self::Continue,
            Self::Reject,
            Self::Accept,
            Self::SendBack,
        ]
    }

    /// The YAML/log spelling — the one place each option's text is
    /// written, for both building an escalation and comparing a choice.
    pub fn as_str(self) -> &'static str {
        match self {
            ReservedOption::Abort => "abort",
            ReservedOption::Retry => "retry",
            ReservedOption::ContinueWork => "continue-work",
            ReservedOption::Promote => "promote",
            ReservedOption::Approve => "approve",
            ReservedOption::Grant => "grant",
            ReservedOption::Deny => "deny",
            ReservedOption::Continue => "continue",
            ReservedOption::Reject => "reject",
            ReservedOption::Accept => "accept",
            ReservedOption::SendBack => "send-back",
        }
    }

    /// Whether choosing this runs the failed node again — from scratch,
    /// from the work it left, with its scope widened first, with a request
    /// it owed refused, so the tasks that asked go on in their scope, or
    /// with a departure it owed answered.
    pub fn runs_again(self) -> bool {
        matches!(
            self,
            ReservedOption::Retry
                | ReservedOption::ContinueWork
                | ReservedOption::Grant
                | ReservedOption::Deny
                | ReservedOption::Accept
                | ReservedOption::SendBack
        )
    }

    /// Whether a task this choice runs again picks its last attempt's work
    /// back up: carrying it on, or carrying it on with a wider scope.
    pub fn continues_work(self) -> bool {
        matches!(self, ReservedOption::ContinueWork | ReservedOption::Grant)
    }

    /// The option as an escalation offers it.
    pub fn id(self) -> OptionId {
        OptionId::from_static(self.as_str())
    }

    /// The reserved option `id` spells, or `None` for an author's own.
    pub fn of(id: &OptionId) -> Option<Self> {
        id.as_str().parse().ok()
    }
}

impl FromStr for ReservedOption {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::all()
            .into_iter()
            .find(|option| option.as_str() == s)
            .ok_or(())
    }
}

impl From<ReservedOption> for String {
    fn from(option: ReservedOption) -> String {
        option.as_str().to_string()
    }
}

impl TryFrom<String> for ReservedOption {
    type Error = String;

    fn try_from(spelling: String) -> Result<Self, Self::Error> {
        spelling
            .parse()
            .map_err(|()| format!("unknown reserved option `{spelling}`"))
    }
}

impl schemars::JsonSchema for ReservedOption {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ReservedOption")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "enum": Self::all().into_iter().map(Self::as_str).collect::<Vec<_>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_option_is_read_back_from_the_id_it_is_offered_as() {
        for option in ReservedOption::all() {
            assert_eq!(
                ReservedOption::of(&option.id()),
                Some(option),
                "{}",
                option.as_str()
            );
        }
    }

    #[test]
    fn a_departure_answer_is_spelled_as_the_log_writes_it() {
        assert_eq!(ReservedOption::Accept.as_str(), "accept");
        assert_eq!(ReservedOption::SendBack.as_str(), "send-back");
        assert_eq!(
            "send-back".parse::<ReservedOption>(),
            Ok(ReservedOption::SendBack)
        );
    }

    #[test]
    fn an_option_is_written_as_its_spelling_and_read_back_from_it() {
        for option in ReservedOption::all() {
            let written = serde_json::to_value(option).unwrap();
            assert_eq!(written, serde_json::Value::from(option.as_str()));
            let read: ReservedOption = serde_json::from_value(written).unwrap();
            assert_eq!(read, option);
        }
        assert!(serde_json::from_value::<ReservedOption>(serde_json::json!("ship-it")).is_err());
    }
}
