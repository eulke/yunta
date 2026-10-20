//! A decision a run waits on: every option, what it costs, and the
//! command that chooses it.

use std::collections::BTreeMap;

use crate::components::Drawn;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap, INDENT};

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
}

/// Where a decision's options are chosen.
pub enum Chosen {
    /// At a terminal, by the command each option names: the command for
    /// each option's id, written when the decision was made for its run.
    Here { commands: BTreeMap<String, String> },
    /// On the forge holding the pull request: each option's label says
    /// what to do there, and no command chooses it.
    OnTheForge,
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

impl Drawn for Decision {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let under = format!("{INDENT}{INDENT}");
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for option in &self.options {
            let mut head = Line::new()
                .plain(INDENT)
                .push(Tone::Strong, option.id.as_str());
            if let Some(label) = option.said_label() {
                head = head.plain(" — ").plain(label);
            }
            lines.push(head);
            let mut said: Vec<String> = option.tradeoff.iter().cloned().collect();
            said.extend(option.asks.iter().map(|asks| format!("asks: {asks}")));
            for part in said.iter().flat_map(|text| wrap(text, room)) {
                lines.push(Line::new().plain(under.as_str()).push(Tone::Muted, part));
            }
            if let Some(command) = self.command(option) {
                lines.push(
                    Line::new()
                        .plain(under.as_str())
                        .push(Tone::Strong, command),
                );
            }
        }
        lines
    }
}
