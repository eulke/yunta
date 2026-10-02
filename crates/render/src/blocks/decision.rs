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

/// A run's decision, every option with the command that chooses it: a
/// reader copies a line rather than composing one from a menu.
pub struct Decision {
    /// What the run is called by.
    pub handle: String,
    pub options: Vec<DecisionOption>,
}

impl Decision {
    /// The command that chooses `option`, with the words it asks for
    /// left for the reader to write.
    pub fn command(&self, option: &DecisionOption) -> String {
        let text = match option.asks {
            Some(_) => " --text \"<answer>\"",
            None => "",
        };
        format!("yunta resolve-gate {} {}{text}", self.handle, option.id)
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
            lines.push(
                Line::new()
                    .plain(under.as_str())
                    .push(Tone::Strong, self.command(option)),
            );
        }
        lines
    }
}
