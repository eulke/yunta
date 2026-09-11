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
//! **One block, two layouts.** `yunta status` opens on the decision and
//! has a page for it; the block a run leaves on the terminal opens on
//! the outcome and carries the decision as a trailer under it. What the
//! block *contains* — the summary, the evidence the engine attached,
//! every option with the tradeoff that makes it a choice, the command
//! that answers it — is decided once, here; [`Layout`] decides only how
//! much room each of those parts gets and what heading sits over it.
//!
//! **Two pauses reconstruct a menu, and only two**: a node whose
//! re-routes are exhausted, and an unresolved internal gate. A budget
//! cap, a scope expansion in ask mode, an unanswered questions artifact
//! and an external gate with no reachable forge park a run with no menu
//! to rebuild — the forge one because whether a forge answers depends
//! on the machine asking, which no log can say. Those runs still report
//! what they are waiting on, and what to do instead of choosing an
//! option.

use yunta_core::events::{GateOption, GateWaitingPayload};
use yunta_core::{NodeId, RunId};

use crate::commands::advice;
use crate::render::{cell_width, wrap, LINE_WIDTH};

/// How far the body of the block sits from the left margin.
const INDENT: &str = "  ";

/// Which shape of the block to draw.
///
/// The two differ in how much room each part is given and in the
/// headings around them, never in which parts there are: the renderer
/// walks the same escalation either way, so a part can only be dropped
/// from both at once.
#[derive(Clone, Copy)]
pub(crate) enum Layout {
    /// A page of its own: every part under its own heading, wrapped to
    /// the width a terminal is taken to have, because a reader who came
    /// to `yunta status` came for exactly this.
    Page,
    /// A trailer under the outcome a run closed with: every part on the
    /// one line it is given, its label inline, so the decision reads as
    /// the end of the block above it rather than as a second page.
    Trailer,
}

/// The block a surface prints for a run parked on a decision: the
/// escalation as the engine built it — summary, mechanical evidence,
/// every option with its mandatory tradeoff — and the command that
/// answers it.
///
/// The command carries the run's own id and leaves the option as
/// `<option>`, because an example option gets pasted: the reader picks
/// one from the menu above it, and no id printed here is ever the one
/// this run's menu does not offer.
pub(crate) fn block(
    layout: Layout,
    run_id: &RunId,
    node: &NodeId,
    escalation: &GateWaitingPayload,
) -> String {
    let mut out = layout.heading(node);
    out.push_str(&layout.lead(&escalation.summary));
    if !escalation.evidence.is_empty() {
        out.push_str(&layout.field("evidence", &escalation.evidence));
    }
    if let Some(external_ref) = &escalation.external_ref {
        out.push_str(&layout.field("published at", external_ref));
    }
    out.push_str(&layout.section("options"));
    for option in &escalation.options {
        out.push_str(&layout.option(option));
    }
    out.push_str(&layout.section("answer it with"));
    out.push_str(&verbatim(2, &advice::resolve_gate(run_id)));
    out.push_str(&layout.aside());
    out
}

impl Layout {
    /// The line the block opens with: the page names the decision it is
    /// a page about, the trailer names the wait the outcome above it
    /// just reported.
    fn heading(self, node: &NodeId) -> String {
        match self {
            Layout::Page => format!("decision needed on node `{node}`:\n"),
            Layout::Trailer => format!("{INDENT}waiting on node `{node}`\n"),
        }
    }

    /// The escalation's own summary, which needs no label: it is the
    /// sentence the whole block is about.
    fn lead(self, summary: &str) -> String {
        match self {
            Layout::Page => paragraph(summary, 1),
            Layout::Trailer => verbatim(2, &one_line(summary)),
        }
    }

    /// One labelled part — the evidence the engine attached, the handle
    /// a gate was published under. The page hangs it under its label;
    /// the trailer keeps the label inline, on the part's own line.
    fn field(self, label: &str, text: &str) -> String {
        match self {
            Layout::Page => format!("{}{}", self.section(label), paragraph(text, 2)),
            Layout::Trailer => verbatim(2, &format!("{label}: {}", one_line(text))),
        }
    }

    /// The heading over a group of parts. A trailer has no room for one:
    /// the options are the only list under it, and the command is the
    /// last line before the aside.
    fn section(self, label: &str) -> String {
        match self {
            Layout::Page => verbatim(1, &format!("{label}:")),
            Layout::Trailer => String::new(),
        }
    }

    /// One option: its id and label on the line a reader scans, its
    /// tradeoff under it. The tradeoff is never dropped — it is what
    /// makes the choice a decision rather than a guess.
    fn option(self, option: &GateOption) -> String {
        let headline = format!("{} — {}", option.id, option.label);
        let tradeoff = format!("tradeoff: {}", option.tradeoff);
        match self {
            Layout::Page => format!("{}{}", paragraph(&headline, 2), paragraph(&tradeoff, 3)),
            Layout::Trailer => format!(
                "{}{}",
                verbatim(2, &one_line(&headline)),
                verbatim(3, &one_line(&tradeoff))
            ),
        }
    }

    /// What a reader can walk away and do, on the block that closes a
    /// run out: a person who has just watched their terminal stop is the
    /// one who needs telling that nothing is holding the answer open. A
    /// page nobody is waiting in front of does not.
    fn aside(self) -> String {
        match self {
            Layout::Page => String::new(),
            Layout::Trailer => verbatim(
                1,
                "the run holds its own state on disk — close this terminal whenever you like \
                 and answer from anywhere.",
            ),
        }
    }
}

/// The block `yunta status` prints for a run parked on something with no
/// menu: what it waits on, that nothing here is answerable by choosing
/// an option, and what moves the run instead.
pub(crate) fn without_menu(run_id: &RunId, reason: &str) -> String {
    let mut out = "waiting on:\n".to_string();
    out.push_str(&paragraph(reason, 1));
    out.push_str(&paragraph(
        "no options to choose here: a menu is reconstructed for an exhausted \
         re-route and for an unresolved gate node, and this pause is neither. \
         Resolve it where it was raised — a budget, a scope, an answers file, \
         a review on the forge — then hand the run back with:",
        1,
    ));
    out.push_str(&verbatim(2, &advice::resume(run_id)));
    out
}

/// One line as it stands, `depth` steps in: what a command gets, since
/// a command broken across lines is a command that does not run.
fn verbatim(depth: usize, text: &str) -> String {
    format!("{}{text}\n", INDENT.repeat(depth))
}

/// `text` as whole lines `depth` steps in, each one inside the width a
/// terminal is taken to have.
///
/// Text with no words in it draws nothing: a block is built out of
/// fields an escalation may leave empty, and an indent on a line of its
/// own is trailing whitespace a reader never asked for.
fn paragraph(text: &str, depth: usize) -> String {
    let indent = INDENT.repeat(depth);
    let room = LINE_WIDTH.saturating_sub(cell_width(&indent));
    wrap(text, room)
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| format!("{indent}{line}\n"))
        .collect()
}

/// Prose collapsed onto the one line a trailer's part is given.
fn one_line(text: &str) -> String {
    yunta_core::text::one_line(text)
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
            resolve_with: advice::resolve_gate(run_id),
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

    /// How many two-space steps a line is indented by — the shape of a
    /// block, read without pinning the prose that fills it.
    fn depth(line: &str) -> usize {
        (line.len() - line.trim_start().len()) / 2
    }

    #[test]
    fn a_trailer_gives_every_part_of_a_decision_the_one_line_it_has() {
        // Nine parts, nine lines, however long the prose in them: the
        // wait, the summary, the evidence, each option with its
        // tradeoff, the command, and the note that nothing has to stay
        // open. A part that wrapped, or a heading over one, would read
        // as a second page under a block that already said its outcome.
        let mut escalation = escalation();
        escalation.summary = "a sentence longer than the width a terminal is taken to \
                              have, so a layout that wrapped it would draw more lines \
                              than there are parts"
            .to_string();
        let trailer = block(Layout::Trailer, &RUN, &NODE, &escalation);
        assert_eq!(
            trailer.lines().map(depth).collect::<Vec<_>>(),
            [1, 2, 2, 2, 3, 2, 3, 2, 1],
            "{trailer}"
        );
    }

    #[test]
    fn a_page_hangs_every_part_of_a_decision_under_a_heading_of_its_own() {
        let page = block(Layout::Page, &RUN, &NODE, &escalation());
        assert!(
            page.starts_with("decision needed on node `lint`:\n"),
            "{page}"
        );
        for heading in ["  evidence:", "  options:", "  answer it with:"] {
            assert!(page.lines().any(|line| line == heading), "{page}");
        }
        assert!(
            !page.contains("close this terminal"),
            "a page nobody is waiting in front of carries no aside: {page}"
        );
    }

    #[test]
    fn every_line_of_a_decision_fits_the_width_a_terminal_is_taken_to_have() {
        let block = block(Layout::Page, &RUN, &NODE, &escalation());
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
        let block = block(Layout::Page, &RUN, &NODE, &escalation());
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
        let block = block(Layout::Page, &RUN, &NODE, &escalation());
        assert!(
            block.contains("yunta resolve-gate 01JBZ5X8K3N7Q2W6E4R9T1Y0P5 <option>"),
            "{block}"
        );
    }
}
