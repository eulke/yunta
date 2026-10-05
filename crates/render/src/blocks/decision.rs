//! A decision a run waits on: every option, what it costs, and the
//! command that chooses it.

use super::Drawn;
use crate::ink::{Line, Tone};
use crate::{cell_width, wrap, Look, INDENT};

/// One option on a decision's menu.
pub struct DecisionOption {
    pub id: String,
    /// What the option does, when it says more than its id.
    pub label: Option<String>,
    /// What choosing it costs — never dropped: it is what makes the
    /// choice a decision and not a guess.
    pub tradeoff: String,
    /// What the option needs said with it, when it needs anything.
    pub asks: Option<String>,
}

/// A run's decision, every option with how it is chosen: at a terminal
/// a reader copies a line rather than composing one from a menu.
pub struct Decision {
    pub chosen: Chosen,
    pub options: Vec<DecisionOption>,
}

/// Where a decision's options are chosen.
pub enum Chosen {
    /// At a terminal, by the command each option names, for the run
    /// called `handle`.
    Here { handle: String },
    /// On the forge holding the pull request: each option's label says
    /// what to do there, and no command chooses it.
    OnTheForge,
}

impl Decision {
    /// The command that chooses `option`, with the words it asks for
    /// left for the reader to write — when a command chooses it.
    pub fn command(&self, option: &DecisionOption) -> Option<String> {
        let Chosen::Here { handle } = &self.chosen else {
            return None;
        };
        let text = match option.asks {
            Some(_) => " --text \"<answer>\"",
            None => "",
        };
        Some(format!("yunta resolve-gate {handle} {}{text}", option.id))
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
            if let Some(label) = &option.label {
                head = head.plain(" — ").plain(label.as_str());
            }
            lines.push(head);
            let mut said = vec![option.tradeoff.clone()];
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
