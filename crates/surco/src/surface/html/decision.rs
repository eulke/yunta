//! A decision as a browser draws it, in the aside beside the page: each option
//! a card that says what it does, what it costs and what it asks, with a
//! button that copies the command choosing it; then the other ways out, which
//! move the run without choosing any option.

use super::text;
use crate::components::{Chosen, Decision, DecisionOption};

/// A decision as a browser draws it: its options as cards, when it has any,
/// then its other ways out under their title. A decision with no options is
/// only what follows them.
pub(super) fn decision(decision: &Decision) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !decision.options.is_empty() {
        parts.extend(forge_line(&decision.chosen));
        parts.extend(
            decision
                .options
                .iter()
                .map(|option| option_card(decision, option)),
        );
    }
    if !decision.otherwise.is_empty() {
        parts.push(format!(
            r#"<p class="otherwise">{}</p>{}"#,
            text::words(decision.otherwise_title()),
            text::steps(&decision.otherwise)
        ));
    }
    parts.join("\n")
}

/// The line that says where a decision's options are chosen, when they are
/// chosen on a forge: a reviewer answers there, not at a terminal.
fn forge_line(chosen: &Chosen) -> Option<String> {
    match chosen {
        Chosen::OnTheForge { forge, .. } => Some(format!(
            r#"<p class="otherwise">on {}</p>"#,
            text::words(forge)
        )),
        Chosen::Here { .. } => None,
    }
}

/// One option as a card: its id in bold, then what it does, what it costs and
/// what it asks, each only where the option has it, and the button that copies
/// the command choosing it, where a command chooses it.
fn option_card(decision: &Decision, option: &DecisionOption) -> String {
    let mut card = format!(r#"<div class="opt"><b>{}</b>"#, text::esc(&option.id));
    if let Some(label) = option.said_label() {
        card.push_str(&format!(
            r#" <span class="does">{}</span>"#,
            text::words(label)
        ));
    }
    if let Some(tradeoff) = &option.tradeoff {
        card.push_str(&format!(
            r#" <span class="costs">{}</span>"#,
            text::words(tradeoff)
        ));
    }
    if let Some(asks) = &option.asks {
        card.push_str(&format!(
            r#" <span class="costs">asks: {}</span>"#,
            text::words(asks)
        ));
    }
    if let Some(command) = decision.command(option) {
        card.push_str(&text::command(&command));
    }
    card.push_str("</div>");
    card
}
