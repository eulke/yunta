//! Every option of a decision, what it costs, and the command that chooses it, if any.

use crate::components::{Chosen, Decision};
use crate::line::{Fill, Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap, INDENT};

impl Decision {
    /// Each option with what it costs and the command that chooses it, if any,
    /// then, under the title `otherwise_title` gives, what moves the run
    /// without choosing one. Every line is filled as a tint: a decision is
    /// what waits on the reader, so it stands apart from the page where a
    /// layer draws 24-bit color.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let under = format!("{INDENT}{INDENT}");
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for option in &self.options {
            let mut head = Line::new()
                .plain(INDENT)
                .push(Tone::Strong, option.id.as_str());
            if let Some(label) = option.said_label() {
                head = head.plain(": ").plain(label);
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
        if !self.otherwise.is_empty() {
            lines.push(
                Line::new()
                    .plain(INDENT)
                    .push(Tone::Muted, self.otherwise_title()),
            );
            lines.extend(super::next::steps(&self.otherwise, &under, look));
        }
        lines
            .into_iter()
            .map(|line| line.filled(Fill::Tint))
            .collect()
    }

    /// The title the other ways out stand under: `or` when each option is
    /// chosen at a terminal, by a command or at its prompt, and what is typed
    /// elsewhere when a forge holds the choice.
    pub(in crate::surface) fn otherwise_title(&self) -> &str {
        match &self.chosen {
            Chosen::Here { .. } | Chosen::AtThePrompt => super::OR,
            Chosen::OnTheForge { elsewhere, .. } => elsewhere.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::components::{Chosen, Decision, DecisionOption};
    use crate::line::Fill;
    use crate::look::Look;

    /// A decision with one option that has a cost, and one way out.
    fn decision() -> Decision {
        Decision {
            chosen: Chosen::AtThePrompt,
            options: vec![DecisionOption {
                id: "approve".to_string(),
                label: None,
                tradeoff: Some("its tasks run".to_string()),
                asks: None,
            }],
            otherwise: vec![("yunta close <run>".to_string(), "closes it for good")],
        }
    }

    /// Every line a decision draws is filled as a tint, its way out included.
    #[test]
    fn every_line_a_decision_draws_is_filled_as_a_tint() {
        let lines = decision().lines(&Look::plain());
        assert!(
            lines.len() > 1,
            "an option and a way out are more than one line"
        );
        assert!(lines.iter().all(|line| line.fill() == Some(Fill::Tint)));
    }
}
