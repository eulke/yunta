//! A decision as Markdown: its options as a table, each a row whose columns
//! say how it is chosen, and its other ways out under the title they stand
//! under.

use super::{cell, cell_code_span, plain_text, steps};
use crate::components::{Chosen, Decision, DecisionOption};

/// A decision as Markdown: its options as a table, when it has any, then its
/// other ways out under their title, each part a blank line from the last.
pub(super) fn decision(decision: &Decision) -> String {
    let mut parts = Vec::new();
    if !decision.options.is_empty() {
        parts.push(table(decision));
    }
    if !decision.otherwise.is_empty() {
        parts.push(format!(
            "{}\n\n{}",
            plain_text(decision.otherwise_title()),
            steps(&decision.otherwise)
        ));
    }
    parts.join("\n\n")
}

/// The options as a table: a header naming its columns, the rule under it,
/// and a row for each option.
fn table(decision: &Decision) -> String {
    let titles = header(&decision.chosen);
    let mut lines = vec![row(&titles), format!("|{}", "---|".repeat(titles.len()))];
    lines.extend(
        decision
            .options
            .iter()
            .map(|option| option_row(decision, option)),
    );
    lines.join("\n")
}

/// The column titles of a decision's table: a command chooses an option here,
/// and a reviewer answers it on the forge the decision is on.
fn header(chosen: &Chosen) -> Vec<String> {
    match chosen {
        Chosen::Here { .. } => ["option", "what it does", "what happens", "command"]
            .into_iter()
            .map(cell)
            .collect(),
        Chosen::OnTheForge { forge, .. } => vec![
            cell("answer"),
            cell(&format!("on {forge}")),
            cell("what happens"),
        ],
    }
}

/// One option as a table row: its id in bold, its label, what happens and
/// what it asks, and the command that chooses it when a command does. Each
/// part is escaped as a cell of its own, and the `<br>` before the asks is
/// written after escaping, so the break stays a break.
fn option_row(decision: &Decision, option: &DecisionOption) -> String {
    let mut happens = option.tradeoff.as_deref().map(cell).unwrap_or_default();
    if let Some(asks) = &option.asks {
        happens.push_str(&format!("<br>asks: {}", cell(asks)));
    }
    let mut cells = vec![
        format!("**{}**", cell(&option.id)),
        cell(option.said_label().unwrap_or("")),
        happens,
    ];
    if matches!(decision.chosen, Chosen::Here { .. }) {
        cells.push(
            decision
                .command(option)
                .map(|command| cell_code_span(&command))
                .unwrap_or_default(),
        );
    }
    row(&cells)
}

/// A table row joining `cells`, each already written as a table cell by
/// `cell`, so none is escaped a second time here.
fn row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}
