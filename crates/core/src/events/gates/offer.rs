//! The options of an escalation: what each one is, where an author's option
//! leads, and what it asks of the person who picks it.

use serde::{Deserialize, Serialize};

use crate::events::LoggedWords;
use crate::ids::{NodeId, OptionId};
use crate::reserved::ReservedOption;

/// One choice in a gate's escalation: `id` is what `chosen_option` names
/// back, and `leads` is where an author's option takes the run. The words a
/// person reads beside it are written where it is shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GateOption {
    pub id: OptionId,
    /// Where an author's option takes the run. Absent on an option the
    /// engine reserves, whose menu says what it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leads: Option<Leads>,
    /// The label, trade-off and question an older log stored with it.
    #[serde(flatten)]
    pub logged: LoggedOptionWords,
}

/// Where an author's gate option takes the run once a person picks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Leads {
    /// Back to `node`, which runs again before the gate asks again;
    /// `resumes` when its session picks the work back up with what the
    /// person says.
    Back { node: NodeId, resumes: bool },
    /// On past the gate, to the nodes that wait on it (none when nothing
    /// does); `settles` when going on settles the run's findings it shows.
    On { next: Vec<NodeId>, settles: bool },
}

/// What an option asks the person who picks it, when an answer without
/// words would change nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asks {
    /// A gate sends the work back to a session: what should change.
    Change,
    /// A departure goes back to the session that declared it: what it
    /// should do instead.
    Instead,
    /// The question an older log stored with the option.
    Logged(LoggedWords),
}

/// The words an older log stored with an option.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoggedOptionWords {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<LoggedWords>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tradeoff: Option<LoggedWords>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asks: Option<LoggedWords>,
}

impl LoggedOptionWords {
    /// The label an older log stored, as written.
    pub fn label(&self) -> Option<&str> {
        self.label.as_ref().map(LoggedWords::as_str)
    }

    /// The trade-off an older log stored, as written.
    pub fn tradeoff(&self) -> Option<&str> {
        self.tradeoff.as_ref().map(LoggedWords::as_str)
    }
}

impl GateOption {
    /// An option the engine reserves, as every menu offers it: its id alone.
    pub fn reserved(option: ReservedOption) -> Self {
        GateOption {
            id: option.id(),
            leads: None,
            logged: LoggedOptionWords::default(),
        }
    }

    /// An author's option, and where it takes the run.
    pub fn leading(id: OptionId, leads: Leads) -> Self {
        GateOption {
            id,
            leads: Some(leads),
            logged: LoggedOptionWords::default(),
        }
    }

    /// What the option asks the person who picks it: the one rule for it.
    pub fn asks(&self) -> Option<Asks> {
        if let Some(asked) = &self.logged.asks {
            return Some(Asks::Logged(asked.clone()));
        }
        match (&self.leads, ReservedOption::of(&self.id)) {
            (Some(Leads::Back { resumes: true, .. }), _) => Some(Asks::Change),
            (None, Some(ReservedOption::SendBack)) => Some(Asks::Instead),
            _ => None,
        }
    }
}
