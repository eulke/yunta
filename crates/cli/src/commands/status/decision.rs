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
//! block *contains* — the summary, the engine's own evidence where it
//! says more than that summary, every option with the tradeoff that
//! makes it a choice, the command that answers it — is decided once,
//! here; [`Layout`] decides only how much room each of those parts gets
//! and what heading sits over it.
//!
//! **Two pauses reconstruct a menu, and only two**: a node whose
//! re-routes are exhausted, and an unresolved internal gate. A budget
//! cap, a scope expansion in ask mode, an unanswered questions artifact
//! and an external gate with no reachable forge park a run with no menu
//! to rebuild — the forge one because whether a forge answers depends
//! on the machine asking, which no log can say. Those runs still report
//! what they are waiting on, and what to do instead of choosing an
//! option.

use std::path::Path;

use yunta_core::events::{GateOption, GateWaitingPayload};
use yunta_core::{NodeId, RunId};

use crate::commands::advice;
use crate::render::{cell_width, evidence, indent, option_headline, option_tradeoff, wrap, INDENT};

/// Where a run works, on the one line every surface says it with: the
/// path whole — it is copied into another terminal, never wrapped — and
/// what it means for a decision about the run.
pub(crate) fn run_tree_line(tree: &Path) -> String {
    yunta_core::text::detailed(
        "the run works in",
        &format!(
            "{} — a node run again starts from what is there",
            tree.display()
        ),
    )
}

/// Which shape of the block to draw.
///
/// The two differ in how much room each part is given and in the
/// headings around them, never in which parts there are: the renderer
/// walks the same escalation either way, so a part can only be dropped
/// from both at once.
#[derive(Clone, Copy)]
pub(crate) enum Layout {
    /// A page of its own: every part under its own heading, wrapped to
    /// `width` cells, because a reader who came to `yunta status` came
    /// for exactly this.
    Page { width: usize },
    /// A trailer under the outcome a run closed with: every part on the
    /// one line it is given, its label inline, so the decision reads as
    /// the end of the block above it rather than as a second page.
    Trailer,
}

/// The block a surface prints for a run parked on a decision: the
/// escalation as the engine built it — summary, the engine's own
/// evidence where it adds to that summary, every option with its
/// mandatory tradeoff — and the command that answers it.
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
    out.push_str(&layout.lead(escalation.summary()));
    out.push_str(&layout.facts("evidence", &evidence(escalation)));
    let shows: Vec<String> = escalation
        .shows()
        .iter()
        .map(|shown| yunta_engine::view_of(shown).display().to_string())
        .collect();
    out.push_str(&layout.facts("what it is about", &shows));
    if let Some(external_ref) = escalation.external_ref() {
        out.push_str(&layout.field("published at", external_ref));
    }
    out.push_str(&layout.section("options"));
    for option in escalation.options() {
        out.push_str(&layout.option(option));
    }
    out.push_str(&layout.section("answer it with"));
    out.push_str(&verbatim(2, &advice::resolve_gate(run_id.handle())));
    out.push_str(&layout.advice());
    out
}

impl Layout {
    /// The line the block opens with: the page names the decision it is
    /// a page about, the trailer names the wait the outcome above it
    /// just reported.
    fn heading(self, node: &NodeId) -> String {
        match self {
            Layout::Page { .. } => format!("decision needed on node `{node}`:\n"),
            Layout::Trailer => format!("{INDENT}waiting on node `{node}`\n"),
        }
    }

    /// The escalation's own summary, which needs no label: it is the
    /// sentence the whole block is about.
    fn lead(self, summary: &str) -> String {
        match self {
            Layout::Page { width } => paragraph(summary, 1, width),
            Layout::Trailer => verbatim(2, &one_line(summary)),
        }
    }

    /// One labelled part — the evidence the engine attached, the handle
    /// a gate was published under. The page hangs it under its label;
    /// the trailer keeps the label inline, on the part's own line.
    fn field(self, label: &str, text: &str) -> String {
        match self {
            Layout::Page { width } => {
                format!("{}{}", self.section(label), paragraph(text, 2, width))
            }
            Layout::Trailer => verbatim(2, &yunta_core::text::detailed(label, &one_line(text))),
        }
    }

    /// A labelled group of facts — the record the engine attached. The
    /// page heads them and lists one to a line; the trailer has one
    /// line for the lot and joins them the way the engine's own
    /// one-line surfaces do. Nothing at all when nothing is attached,
    /// so no heading promises a reader something to read.
    fn facts(self, label: &str, facts: &[String]) -> String {
        if facts.is_empty() {
            return String::new();
        }
        match self {
            Layout::Page { width } => {
                let mut out = self.section(label);
                for fact in facts {
                    out.push_str(&paragraph(fact, 2, width));
                }
                out
            }
            Layout::Trailer => self.field(label, &facts.join("; ")),
        }
    }

    /// The heading over a group of parts. A trailer has no room for one:
    /// the options are the only list under it, and the command is the
    /// last line before the advice.
    fn section(self, label: &str) -> String {
        match self {
            Layout::Page { .. } => verbatim(1, &format!("{label}:")),
            Layout::Trailer => String::new(),
        }
    }

    /// One option: its id and label on the line a reader scans, its
    /// tradeoff under it. The tradeoff is never dropped — it is what
    /// makes the choice a decision rather than a guess.
    fn option(self, option: &GateOption) -> String {
        let headline = option_headline(option);
        // What the option asks is said with the answer, which from here
        // is the command's `--text`.
        let tradeoff = match &option.asks {
            Some(asks) => format!(
                "{} — asks: {asks} (say it with --text)",
                option_tradeoff(option)
            ),
            None => option_tradeoff(option),
        };
        match self {
            Layout::Page { width } => format!(
                "{}{}",
                paragraph(&headline, 2, width),
                paragraph(&tradeoff, 3, width)
            ),
            Layout::Trailer => format!(
                "{}{}",
                verbatim(2, &one_line(&headline)),
                verbatim(3, &one_line(&tradeoff))
            ),
        }
    }

    /// What a reader can walk away and do, on the block that closes a
    /// run out. Named for what it says, not for the shape it is laid
    /// out in: `yunta_core::text::aside` is the shape every sentence in
    /// this binary takes, and a method of the same name here would read
    /// as that shape rather than as this content.
    /// a person who has just watched their terminal stop is the
    /// one who needs telling that nothing is holding the answer open. A
    /// page nobody is waiting in front of does not.
    fn advice(self) -> String {
        match self {
            Layout::Page { .. } => String::new(),
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
/// an option, and what moves the run instead, wrapped to `width` cells.
pub(crate) fn without_menu(run_id: &RunId, reason: &str, width: usize) -> String {
    let mut out = "waiting on:\n".to_string();
    out.push_str(&paragraph(reason, 1, width));
    out.push_str(&paragraph(
        "no options to choose here: a menu is reconstructed for a failed node, \
         an exhausted re-route and an unresolved gate node, and this pause is \
         none of them. Resolve it where it was raised — a budget, a scope, an \
         answers file, a review on the forge — then hand the run back with:",
        1,
        width,
    ));
    out.push_str(&verbatim(2, &advice::resume(run_id.handle())));
    out
}

/// One line as it stands, `depth` steps in: what a command gets, since
/// a command broken across lines is a command that does not run.
fn verbatim(depth: usize, text: &str) -> String {
    format!("{}{text}\n", indent(depth))
}

/// `text` as whole lines `depth` steps in, each one inside `width`
/// cells.
///
/// Text with no words in it draws nothing: a block is built out of
/// fields an escalation may leave empty, and an indent on a line of its
/// own is trailing whitespace a reader never asked for.
fn paragraph(text: &str, depth: usize, width: usize) -> String {
    let margin = indent(depth);
    let room = width.saturating_sub(cell_width(&margin));
    wrap(text, room)
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| format!("{margin}{line}\n"))
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
            resolve_with: advice::resolve_gate(run_id.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{Escalation, Evidence, Fact};
    use yunta_core::NonEmpty;

    use super::*;
    use crate::render::LINE_WIDTH;

    /// A page laid out off a terminal.
    const PAGE: Layout = Layout::Page { width: LINE_WIDTH };

    fn escalation() -> GateWaitingPayload {
        escalation_saying("node `lint` failed and its 0 re-routes to `fix-lint` are exhausted")
    }

    /// The same escalation with a claim of the caller's choosing — for
    /// a layout test about how long a summary may run.
    fn escalation_saying(summary: &str) -> GateWaitingPayload {
        Escalation::new(
            summary,
            vec![Fact::bare("exit 1")].into(),
            NonEmpty::from((
                GateOption {
                    id: yunta_core::OptionId::from_static("retry"),
                    label: "Re-route to `fix-lint` once more".to_string(),
                    tradeoff: "Uses one extra correction attempt beyond the declared \
                               max_reroutes (0); escalates again if `fix-lint` doesn't \
                               fix it"
                        .to_string(),
                    asks: None,
                },
                vec![GateOption {
                    id: yunta_core::OptionId::from_static("abort"),
                    label: "Abort the run".to_string(),
                    tradeoff: "Pauses here; nothing further executes".to_string(),
                    asks: None,
                }],
            )),
        )
        .expect("the summary states no fact the evidence holds")
        .into_payload()
    }

    /// An unresolved internal gate, the other pause a menu is rebuilt
    /// for: it asks with the author's own message, and the evidence
    /// under it — who the gate is assigned to — appears nowhere in that
    /// message.
    fn gate() -> GateWaitingPayload {
        Escalation::new(
            "Approve the plan?",
            vec![Fact::labelled("assignee", "lead")].into(),
            NonEmpty::from((
                GateOption {
                    id: yunta_core::OptionId::from_static("approve"),
                    label: "approve".to_string(),
                    tradeoff: "resolves this gate; the flow continues".to_string(),
                    asks: None,
                },
                Vec::new(),
            )),
        )
        .expect("the summary states no fact the evidence holds")
        .into_payload()
    }

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const NODE: NodeId = NodeId::from_static("lint");
    const GATE: NodeId = NodeId::from_static("approve");

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
        // A summary that says nothing about the exit code, so the
        // evidence under it is a part of its own.
        let escalation = escalation_saying(
            "a sentence longer than the width a terminal is taken to have, so a layout \
             that wrapped it would draw more lines than there are parts",
        );
        let trailer = block(Layout::Trailer, &RUN, &NODE, &escalation);
        assert_eq!(
            trailer.lines().map(depth).collect::<Vec<_>>(),
            [1, 2, 2, 2, 3, 2, 3, 2, 1],
            "{trailer}"
        );
    }

    #[test]
    fn a_page_hangs_every_part_of_a_decision_under_a_heading_of_its_own() {
        let page = block(PAGE, &RUN, &NODE, &escalation());
        assert!(
            page.starts_with("decision needed on node `lint`:\n"),
            "{page}"
        );
        for heading in ["  options:", "  answer it with:"] {
            assert!(page.lines().any(|line| line == heading), "{page}");
        }
        assert!(
            !page.contains("close this terminal"),
            "a page nobody is waiting in front of carries no advice: {page}"
        );
    }

    #[test]
    fn a_page_hangs_evidence_under_its_own_heading_when_it_says_more_than_the_summary() {
        let page = block(PAGE, &RUN, &GATE, &gate());
        assert!(page.lines().any(|line| line == "  evidence:"), "{page}");
        assert!(page.contains("assignee: lead"), "{page}");
    }

    #[test]
    fn the_claim_and_the_record_behind_it_are_each_said_once() {
        // The summary is what happened and the evidence is what the log
        // says about it. A reader audits the first against the second,
        // which only works while neither one is a copy of the other.
        for layout in [PAGE, Layout::Trailer] {
            let drawn = block(layout, &RUN, &NODE, &escalation());
            assert_eq!(
                drawn.matches("exit 1").count(),
                1,
                "the cause is attached as the record, not repeated into the claim: {drawn}"
            );
            assert!(
                drawn.contains("are exhausted"),
                "the claim says what happened: {drawn}"
            );
        }
    }

    #[test]
    fn an_escalation_with_nothing_attached_heads_no_record() {
        let bare = Escalation::new(
            "node `lint` failed and its 0 re-routes to `fix-lint` are exhausted",
            Evidence::none(),
            NonEmpty::from((
                GateOption {
                    id: yunta_core::OptionId::from_static("abort"),
                    label: "Abort the run".to_string(),
                    tradeoff: "Pauses here; nothing further executes".to_string(),
                    asks: None,
                },
                Vec::new(),
            )),
        )
        .expect("an escalation with nothing attached repeats nothing")
        .into_payload();
        for layout in [PAGE, Layout::Trailer] {
            let drawn = block(layout, &RUN, &NODE, &bare);
            assert!(
                !drawn.contains("evidence"),
                "no heading promises a record that is not there: {drawn}"
            );
        }
    }

    #[test]
    fn where_the_run_works_is_said_with_its_path_whole() {
        let line = run_tree_line(std::path::Path::new("/state/worktrees/run-1"));
        assert_eq!(
            line,
            "the run works in: /state/worktrees/run-1 — a node run again starts from what is there"
        );
    }

    #[test]
    fn every_line_of_a_decision_fits_the_width_a_terminal_is_taken_to_have() {
        let block = block(PAGE, &RUN, &NODE, &escalation());
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
        let block = block(PAGE, &RUN, &NODE, &escalation());
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
            block.contains("Pauses here; nothing further executes"),
            "{block}"
        );
    }

    #[test]
    fn the_command_leaves_the_option_for_the_reader_to_choose() {
        let block = block(PAGE, &RUN, &NODE, &escalation());
        assert!(
            block.contains("yunta resolve-gate T1Y0P5 <option>"),
            "{block}"
        );
    }
}
