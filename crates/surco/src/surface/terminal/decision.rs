//! Every option of a decision, what it costs, and the command that chooses it, if any.

use crate::components::{Chosen, Decision, DecisionOption};
use crate::line::{Fill, Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap, INDENT};

impl Decision {
    /// Each option with what it costs and the command that chooses it, if any,
    /// then, under the title `otherwise_title` gives, what moves the run
    /// without choosing one. A decision is what waits on the reader, so every
    /// line opens with its edge, in the person's tone and in every layer, in
    /// place of the line's first two spaces, and is filled as a tint where a
    /// layer draws 24-bit color.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let under = format!("{INDENT}{INDENT}");
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for option in &self.options {
            lines.extend(head(option, &under, room));
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
        let edge = Line::new()
            .push(Tone::NeedsYou, look.glyphs.decision_edge().to_string())
            .plain(" ");
        lines
            .into_iter()
            .map(|line| {
                let rest = line.slice(INDENT.len()..line.text().len());
                edge.clone().then(rest).filled(Fill::Tint)
            })
            .collect()
    }

    /// The title the other ways out stand under: `or` when each option is
    /// chosen at a terminal by a command, and what is typed elsewhere when a
    /// forge holds the choice.
    pub(in crate::surface) fn otherwise_title(&self) -> &str {
        match &self.chosen {
            Chosen::Here { .. } => super::OR,
            Chosen::OnTheForge { elsewhere, .. } => elsewhere.as_str(),
        }
    }
}

/// An option's id, in the strong tone, and its label, wrapped under it where
/// the line runs out.
fn head(option: &DecisionOption, under: &str, room: usize) -> Vec<Line> {
    let id = option.id.as_str();
    let label = option.said_label().map(|label| format!(": {label}"));
    let said = format!("{id}{}", label.unwrap_or_default());
    wrap(&said, room)
        .into_iter()
        .enumerate()
        .map(|(at, part)| match at {
            0 => {
                let rest = part.strip_prefix(id).unwrap_or_default().to_string();
                Line::new()
                    .plain(INDENT)
                    .push(Tone::Strong, id)
                    .plain(rest)
            }
            _ => Line::new().plain(under).plain(part),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::components::{Chosen, Decision, DecisionOption};
    use crate::line::{Fill, Line};
    use crate::look::Look;

    /// A decision with one option that has a cost, and one way out.
    fn decision() -> Decision {
        Decision {
            chosen: Chosen::Here {
                commands: BTreeMap::new(),
            },
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

    /// Every line a decision draws opens with its edge, in the person's tone,
    /// in place of its first two spaces, so each line keeps its width.
    #[test]
    fn a_label_wider_than_its_line_wraps_under_its_option() {
        let mut decision = decision();
        decision.options[0].label =
            Some("Run it again (attempt 2) once something outside the tree changes".to_string());
        let look = Look {
            width: crate::look::Width::of(Some(60), None),
            ..Look::plain()
        };
        let drawn: Vec<String> = decision.lines(&look).iter().map(Line::text).collect();
        assert_eq!(
            drawn[0],
            "| approve: Run it again (attempt 2) once something outside"
        );
        assert_eq!(drawn[1], "|   the tree changes");
        for line in &drawn {
            assert!(crate::width::cell_width(line) <= 60, "{line}");
        }
    }

    #[test]
    fn every_line_a_decision_draws_opens_with_its_edge() {
        let look = Look::plain();
        let edge = look.glyphs.decision_edge();
        let lines = decision().lines(&look);
        assert_eq!(lines[0].text(), format!("{edge} approve"));
        for line in &lines {
            assert!(
                line.text().starts_with(&format!("{edge} ")),
                "{}",
                line.text()
            );
            assert_eq!(line.spans()[0].tone, crate::line::Tone::NeedsYou);
        }
    }
}
