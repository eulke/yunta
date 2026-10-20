//! Every option of a decision, what it costs, and the command that chooses it.

use crate::components::Decision;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap, INDENT};

impl Decision {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
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
