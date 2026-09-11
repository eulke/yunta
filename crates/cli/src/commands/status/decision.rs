//! The decision a parked run is waiting on, as a person reads it.
//!
//! A run stops on a person from a process that is already gone: the menu
//! it stopped on was never written to the log, because building it is a
//! computation, not a fact anyone recorded.
//! `yunta_engine::current_escalation` rebuilds that menu from the
//! manifest and the log alone, which is what lets a second terminal
//! answer a run it never started — and what this prints, so the answer
//! does not have to be guessed from the exported JSONL.
//!
//! **Two shapes reconstruct, and only two**: a node whose re-routes are
//! exhausted, and an unresolved internal gate. A budget cap, a scope
//! expansion in ask mode, an unanswered questions artifact and an
//! external gate with no reachable forge park a run with no menu to
//! rebuild — the forge one because whether a forge answers depends on
//! the machine asking, which no log can say. Those runs still report
//! what they are waiting on, and what to do instead of choosing an
//! option.

use yunta_core::events::{GateOption, GateWaitingPayload};
use yunta_core::{NodeId, RunId};

use crate::render::{cell_width, LINE_WIDTH};

/// How far the body of the block sits from the left margin.
const INDENT: &str = "  ";

/// The block `yunta status` prints for a run parked on a decision: the
/// escalation as the engine built it — summary, mechanical evidence,
/// every option with its mandatory tradeoff — and the command that
/// answers it.
///
/// The command carries the run's own id and leaves the option as
/// `<option>`, because an example option gets pasted: the reader picks
/// one from the menu above it, and no id printed here is ever the one
/// this run's menu does not offer.
pub(crate) fn block(run_id: &RunId, node: &NodeId, escalation: &GateWaitingPayload) -> String {
    let mut out = format!("decision needed on node `{node}`:\n");
    out.push_str(&paragraph(&escalation.summary, INDENT));
    if !escalation.evidence.is_empty() {
        out.push_str(&format!("{INDENT}evidence:\n"));
        out.push_str(&paragraph(
            &escalation.evidence,
            &format!("{INDENT}{INDENT}"),
        ));
    }
    if let Some(external_ref) = &escalation.external_ref {
        out.push_str(&format!("{INDENT}published at:\n"));
        out.push_str(&paragraph(external_ref, &format!("{INDENT}{INDENT}")));
    }
    out.push_str(&format!("{INDENT}options:\n"));
    for option in &escalation.options {
        out.push_str(&option_block(option));
    }
    out.push_str(&format!("{INDENT}answer it with:\n"));
    out.push_str(&format!("{INDENT}{INDENT}{}\n", resolve_command(run_id)));
    out
}

/// The block `yunta status` prints for a run parked on something with no
/// menu: what it waits on, that nothing here is answerable by choosing
/// an option, and what moves the run instead.
pub(crate) fn without_menu(run_id: &RunId, reason: &str) -> String {
    let mut out = "waiting on:\n".to_string();
    out.push_str(&paragraph(reason, INDENT));
    out.push_str(&paragraph(
        "no options to choose here: a menu is reconstructed for an exhausted \
         re-route and for an unresolved gate node, and this pause is neither. \
         Resolve it where it was raised — a budget, a scope, an answers file, \
         a review on the forge — then hand the run back with:",
        INDENT,
    ));
    out.push_str(&format!(
        "{INDENT}{INDENT}yunta resume {}\n",
        run_id.as_str()
    ));
    out
}

/// The command that answers a reconstructed decision, with the option
/// left as a placeholder. One place, so the block a person reads and the
/// `resolve_with` a program reads are the same string.
pub(crate) fn resolve_command(run_id: &RunId) -> String {
    format!("yunta resolve-gate {} <option>", run_id.as_str())
}

/// One option: its id and label on a line a reader scans, its tradeoff
/// under it. The tradeoff is never dropped — it is what makes the choice
/// a decision rather than a guess.
fn option_block(option: &GateOption) -> String {
    let mut out = paragraph(
        &format!("{} — {}", option.id, option.label),
        &format!("{INDENT}{INDENT}"),
    );
    out.push_str(&paragraph(
        &format!("tradeoff: {}", option.tradeoff),
        &format!("{INDENT}{INDENT}{INDENT}"),
    ));
    out
}

/// `text` as whole lines under `indent`, each one inside the width a
/// terminal is taken to have.
fn paragraph(text: &str, indent: &str) -> String {
    let room = LINE_WIDTH.saturating_sub(cell_width(indent));
    wrap(text, room)
        .into_iter()
        .map(|line| format!("{indent}{line}\n"))
        .collect()
}

/// `text` as lines of at most `width` display cells, broken between
/// words.
///
/// A word with no room of its own — a published gate's URL — is carried
/// across lines rather than cut: a reader needs all of it, and a line
/// that ran past the width would be re-wrapped by the terminal at a
/// place this function does not choose.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in yunta_core::text::one_line(text).split(' ') {
        for piece in pieces(word, width) {
            let room = width.saturating_sub(cell_width(&line));
            if line.is_empty() {
                line = piece;
            } else if cell_width(&piece) < room {
                line.push(' ');
                line.push_str(&piece);
            } else {
                lines.push(std::mem::take(&mut line));
                line = piece;
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// `word` in the pieces a line of `width` cells can hold: the word
/// itself when it fits, and the chunks it fills otherwise.
fn pieces(word: &str, width: usize) -> Vec<String> {
    if cell_width(word) <= width {
        return vec![word.to_string()];
    }
    let mut pieces = Vec::new();
    let mut piece = String::new();
    for ch in word.chars() {
        if cell_width(&piece) + cell_width(&ch.to_string()) > width {
            pieces.push(std::mem::take(&mut piece));
        }
        piece.push(ch);
    }
    if !piece.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// A parked run's decision as the versioned JSON carries it: the node it
/// belongs to, the escalation's own fields under the names the
/// `gate_waiting` event writes them with, and the command that answers
/// it.
#[derive(serde::Serialize)]
pub(crate) struct DecisionJson {
    /// The node `resolve_gate` records the answer against.
    node: String,
    #[serde(flatten)]
    escalation: GateWaitingPayload,
    /// The command a person runs to answer, with `<option>` left for one
    /// of the ids above it.
    resolve_with: String,
}

impl DecisionJson {
    pub(crate) fn new(run_id: &RunId, node: &NodeId, escalation: GateWaitingPayload) -> Self {
        DecisionJson {
            node: node.to_string(),
            escalation,
            resolve_with: resolve_command(run_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escalation() -> GateWaitingPayload {
        GateWaitingPayload {
            summary: "node `lint` failed and its 0 re-route(s) to `fix-lint` are \
                      exhausted: exit 1"
                .to_string(),
            evidence: "exit 1".to_string(),
            options: vec![
                GateOption {
                    id: yunta_core::OptionId::from_static("retry"),
                    label: "Re-route to `fix-lint` once more".to_string(),
                    tradeoff: "Uses one extra correction attempt beyond the declared \
                               max_reroutes (0); escalates again if `fix-lint` doesn't \
                               fix it"
                        .to_string(),
                },
                GateOption {
                    id: yunta_core::OptionId::from_static("abort"),
                    label: "Abort the run".to_string(),
                    tradeoff: "Stops here; nothing further executes".to_string(),
                },
            ],
            external_ref: None,
        }
    }

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const NODE: NodeId = NodeId::from_static("lint");

    #[test]
    fn every_line_of_a_decision_fits_the_width_a_terminal_is_taken_to_have() {
        let block = block(&RUN, &NODE, &escalation());
        for line in block.lines() {
            assert!(
                cell_width(line) <= LINE_WIDTH,
                "{} cells: {line:?}",
                cell_width(line)
            );
        }
    }

    #[test]
    fn a_decision_names_every_option_with_its_tradeoff() {
        let block = block(&RUN, &NODE, &escalation());
        assert!(
            block.contains("retry — Re-route to `fix-lint` once more"),
            "{block}"
        );
        assert!(block.contains("abort — Abort the run"), "{block}");
        assert_eq!(
            block.matches("tradeoff:").count(),
            2,
            "one tradeoff per option: {block}"
        );
        assert!(
            block.contains("Stops here; nothing further executes"),
            "{block}"
        );
    }

    #[test]
    fn the_command_leaves_the_option_for_the_reader_to_choose() {
        let block = block(&RUN, &NODE, &escalation());
        assert!(
            block.contains("yunta resolve-gate 01JBZ5X8K3N7Q2W6E4R9T1Y0P5 <option>"),
            "{block}"
        );
    }

    #[test]
    fn a_word_longer_than_the_line_is_carried_whole_across_it() {
        let url = "https://forge.example/very/long/path/that/never/fits/on/one/line/of/eighty/cells/pull/7";
        let lines = wrap(url, 20);
        assert!(lines.iter().all(|line| cell_width(line) <= 20), "{lines:?}");
        assert_eq!(lines.concat(), url, "every character survives the wrap");
    }
}
