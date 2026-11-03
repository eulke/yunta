//! A decision a run waits on: every option, what it costs, and how it is
//! chosen.

use std::collections::BTreeMap;

/// One option on a decision's menu.
pub struct DecisionOption {
    pub id: String,
    /// What the option does. A label that says no more than the id is not
    /// drawn, so a caller can pass the words an option is read with.
    pub label: Option<String>,
    /// What choosing it costs — what makes the choice a decision and not a
    /// guess. Every option the engine offers carries one.
    pub tradeoff: Option<String>,
    /// What the option needs said with it, when it needs anything.
    pub asks: Option<String>,
}

impl DecisionOption {
    /// The label a menu prints after the id: nothing when the label says no
    /// more than the id.
    pub fn said_label(&self) -> Option<&str> {
        self.label
            .as_deref()
            .filter(|label| *label != self.id.as_str())
    }
}

/// A run's decision, every option with how it is chosen: at a terminal
/// a reader copies a line rather than composing one from a menu.
pub struct Decision {
    pub chosen: Chosen,
    pub options: Vec<DecisionOption>,
    /// What moves the run without choosing any option — closing it,
    /// handing it back once what stopped it is settled elsewhere — each
    /// command with what it does: the other ways out, said after the
    /// options.
    pub otherwise: Vec<(String, &'static str)>,
}

/// Where a decision's options are chosen.
pub enum Chosen {
    /// At a terminal, by the command each option names: the command for
    /// each option's id, written when the decision was made for its run.
    Here { commands: BTreeMap<String, String> },
    /// On the forge that holds the pull request, which `forge` names: each
    /// option's label says what a reviewer does there, and no command
    /// chooses it. What follows the options is typed `elsewhere`, never on
    /// the forge.
    OnTheForge { forge: String, elsewhere: String },
}

impl Decision {
    /// The command that chooses `option`, with the words it asks for
    /// left for the reader to write — when a command chooses it.
    pub fn command(&self, option: &DecisionOption) -> Option<String> {
        let Chosen::Here { commands } = &self.chosen else {
            return None;
        };
        commands.get(&option.id).cloned()
    }
}
