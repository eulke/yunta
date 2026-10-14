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
//! **One block, wherever it is shown.** `yunta status` and the block a
//! run leaves on the terminal show the same decision the same way: what
//! it is about, every option with the tradeoff that makes it a choice,
//! and the command that chooses it. What a surface already says beside
//! it — the claim on its second line, a failed node's evidence above — it
//! does not say twice.
//!
//! **Two pauses reconstruct a menu, and only two**: a node whose
//! re-routes are exhausted, and an unresolved internal gate. A budget
//! cap, a scope expansion in ask mode, an unanswered questions artifact
//! and an external gate with no reachable forge park a run with no menu
//! to rebuild — the forge one because whether a forge answers depends
//! on the machine asking, which no log can say. Those runs still report
//! what they are waiting on, and what to do instead of choosing an
//! option.

use std::collections::BTreeMap;
use std::path::Path;

use yunta_core::events::GateWaitingPayload;
use yunta_core::step::NextStep;
use yunta_core::{NodeId, RunId};
use yunta_engine::ShownDocument;

use crate::commands::advice;
use crate::error::Reader;
use crate::render::blocks::{Decision, DecisionOption, Drawn};
use crate::render::ink::{Line, Tone};
use crate::render::{cell_width, evidence, indent, wrap, Look, Width, INDENT};

/// Where a run works, as the page says it above a decision: the path
/// whole — it is copied into another terminal, never wrapped — and what
/// it means for a decision about the run.
pub(crate) fn run_tree_line(tree: &Path) -> String {
    yunta_core::text::detailed(
        "the run works in",
        &format!(
            "{} — a node run again starts from what is there",
            tree.display()
        ),
    )
}

/// What the surface showing a decision says beside it already.
#[derive(Clone, Copy)]
pub(crate) struct Beside {
    /// The claim the escalation makes — a page whose second line is the
    /// run's pause already said it.
    pub(crate) claim: bool,
    /// The record the engine attached — a page that quotes a failed
    /// node's evidence already shows it.
    pub(crate) evidence: bool,
}

/// The lines a decision reads as: what it is on, what it is about, what
/// weighs on it, and every option with what it costs and the command that
/// chooses it.
pub(crate) fn lines(
    (run_id, node): (&RunId, &NodeId),
    escalation: &GateWaitingPayload,
    shown: &[ShownDocument],
    beside: Beside,
    look: &Look,
) -> Vec<Line> {
    let mut lines = vec![Line::new()
        .plain(INDENT)
        .push(Tone::Strong, format!("decision on node `{node}`"))];
    lines.extend(account(escalation, beside, 2, look));
    for shown in escalation.shows() {
        let about = format!("about: {}", yunta_engine::view_of(shown).display());
        lines.extend(under(&about, Tone::Plain, 2, look));
    }
    if let Some(external_ref) = escalation.external_ref() {
        lines.extend(under(
            &format!("published at {external_ref}"),
            Tone::Plain,
            2,
            look,
        ));
    }
    let before = before_you_decide(escalation, shown, look);
    if !before.is_empty() {
        // What weighs on the decision ends a block of its own, apart from
        // the options it weighs on.
        lines.extend(before);
        lines.push(Line::new());
    }
    lines.extend(menu(run_id, escalation, look));
    lines
}

/// What weighs on a decision, said between its question and its options:
/// what the gate does not offer and why, and what a person should know
/// of what it shows. Nothing, with a blank line above it, when there is
/// nothing to say.
pub(crate) fn before_you_decide(
    escalation: &GateWaitingPayload,
    shown: &[ShownDocument],
    look: &Look,
) -> Vec<Line> {
    let Some(before) = crate::render::shown::before_you_decide(escalation.withheld(), shown) else {
        return Vec::new();
    };
    let mut lines = vec![Line::new()];
    lines.extend(crate::render::surface_lines(
        crate::render::doc::Doc::new().with(before),
        look,
    ));
    lines
}

/// What a decision says happened, `depth` steps under its heading: the
/// claim, and the record the engine attached to audit it against — less
/// what the surface already says beside it.
pub(crate) fn account(
    escalation: &GateWaitingPayload,
    beside: Beside,
    depth: usize,
    look: &Look,
) -> Vec<Line> {
    let mut lines = Vec::new();
    if !beside.claim {
        lines.extend(under(&claim_words(escalation), Tone::Plain, depth, look));
    }
    if !beside.evidence {
        for fact in evidence(escalation) {
            lines.extend(under(&fact, Tone::Muted, depth, look));
        }
    }
    lines
}

/// The claim of a decision in the words a reader sees: what the escalation
/// says happened, with the re-route a failed node has used up named in it.
fn claim_words(escalation: &GateWaitingPayload) -> String {
    yunta_render::claim::sentence(escalation.summary(), escalation.failure.as_deref())
}

/// `text` in `tone`, wrapped `depth` steps in.
fn under(text: &str, tone: Tone, depth: usize, look: &Look) -> Vec<Line> {
    let margin = indent(depth);
    let room = look.width.cells().saturating_sub(cell_width(&margin));
    wrap(text, room)
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| Line::new().plain(margin.as_str()).push(tone, line))
        .collect()
}

/// Every option of a decision, one step in: what it costs and the
/// command that chooses it.
fn menu(run_id: &RunId, escalation: &GateWaitingPayload, look: &Look) -> Vec<Line> {
    let options = escalation
        .options()
        .iter()
        .map(|option| {
            let (label, tradeoff) = yunta_render::escalation::option_words(escalation, option);
            DecisionOption {
                id: option.id.to_string(),
                label: Some(label),
                tradeoff,
                asks: option.asks.clone(),
            }
        })
        .collect();
    let menu = Decision::here(run_id, options, &advice::spell);
    let within = Look {
        width: Width::of(Some(look.width.cells().saturating_sub(INDENT.len())), None),
        ..*look
    };
    menu.lines(&within)
        .into_iter()
        .map(|line| line.under(INDENT))
        .collect()
}

/// The lines `yunta status` shows for a run parked on something with no
/// menu: what it waits on, that nothing here is answerable by choosing
/// an option, and what moves the run instead, wrapped to `width` cells.
/// A run waiting on a limit's reset moves on its own, so the lines say
/// that and name no command.
pub(crate) fn without_menu(
    run_id: &RunId,
    reason: &str,
    awaits_reset: bool,
    width: usize,
) -> Vec<Line> {
    let mut lines = vec![Line::new().plain("waiting on:")];
    lines.extend(paragraph(reason, 1, width));
    if awaits_reset {
        lines.extend(paragraph(
            "nothing here needs a choice: the run resumes on its own once the limit resets.",
            1,
            width,
        ));
        return lines;
    }
    lines.extend(paragraph(
        "no options to choose here: a menu is reconstructed for a failed node, \
         an exhausted re-route and an unresolved gate node, and this pause is \
         none of them. Resolve it where it was raised — a budget, a scope, an \
         answers file, a review on the forge — then hand the run back with:",
        1,
        width,
    ));
    lines.push(Line::new().plain(format!("{}{}", indent(2), advice::resume(run_id.handle()))));
    lines
}

/// `text` as whole lines `depth` steps in, each one inside `width`
/// cells. Text with no words in it draws nothing.
fn paragraph(text: &str, depth: usize, width: usize) -> Vec<Line> {
    let margin = indent(depth);
    let room = width.saturating_sub(cell_width(&margin));
    wrap(text, room)
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| Line::new().plain(format!("{margin}{line}")))
        .collect()
}

/// A parked run's decision as the versioned JSON carries it: the node it
/// belongs to, the escalation's own fields under the names the
/// `gate_waiting` event writes them with, the claim as the words a person
/// reads, and the commands that answer it.
#[derive(serde::Serialize)]
pub(crate) struct DecisionJson {
    /// The node `resolve_gate` records the answer against.
    node: String,
    #[serde(flatten)]
    escalation: GateWaitingPayload,
    /// The command a person runs to answer, with `<option>` left for one
    /// of the ids above it.
    resolve_with: String,
    /// The command that chooses each option, by its id.
    commands: BTreeMap<String, String>,
}

impl DecisionJson {
    pub(crate) fn new(run_id: &RunId, node: &NodeId, escalation: GateWaitingPayload) -> Self {
        // The options carry the words the text menu draws, so a program reads
        // what a person reads, including for an option that stores none.
        let escalation = escalation.worded(yunta_render::escalation::option_words);
        // The claim is a string here whatever kind of claim the log holds,
        // so the document says it in the words a person reads.
        let said = claim_words(&escalation);
        let escalation = escalation.with_claim(said);
        let commands = escalation
            .options()
            .iter()
            .map(|option| {
                let id = option.id.to_string();
                // The command names the option alone: the option's `asks` says
                // whether it takes an answer.
                let step = NextStep::ResolveGate {
                    run: run_id.clone(),
                    option: Some(option.id.clone()),
                    text: false,
                };
                (id, advice::spell_for(&step, Reader::Agent))
            })
            .collect();
        DecisionJson {
            node: node.to_string(),
            escalation,
            resolve_with: advice::resolve_gate(run_id.as_str()),
            commands,
        }
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{Claim, Escalation, Evidence, Fact, GateOption};
    use yunta_core::NonEmpty;

    use super::*;
    use crate::render::doc::{Block, Doc};

    fn option(id: &'static str, label: &str, tradeoff: &str) -> GateOption {
        GateOption {
            id: yunta_core::OptionId::from_static(id),
            label: Some(label.to_string()),
            tradeoff: Some(tradeoff.to_string()),
            asks: None,
        }
    }

    fn escalation() -> GateWaitingPayload {
        Escalation::new(
            "node `lint` failed and its 0 re-routes to `fix-lint` are exhausted",
            vec![Fact::bare("exit 1")].into(),
            NonEmpty::from((
                option(
                    "retry",
                    "Re-route to `fix-lint` once more",
                    "Uses one extra correction attempt beyond the declared max_reroutes (0); \
                     escalates again if `fix-lint` doesn't fix it",
                ),
                vec![option(
                    "abort",
                    "Abort the run",
                    "Pauses here; nothing further executes",
                )],
            )),
        )
        .expect("the summary states no fact the evidence holds")
        .into_payload()
    }

    /// A gate whose option an older binary labelled with its own id.
    fn gate() -> GateWaitingPayload {
        Escalation::new(
            "Approve the plan?",
            vec![Fact::labelled("assignee", "lead")].into(),
            NonEmpty::from((
                option(
                    "approve",
                    "approve",
                    "resolves this gate; the flow continues",
                ),
                Vec::new(),
            )),
        )
        .expect("the summary states no fact the evidence holds")
        .into_payload()
    }

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const NODE: NodeId = NodeId::from_static("lint");

    const NOTHING_BESIDE: Beside = Beside {
        claim: false,
        evidence: false,
    };

    fn drawn(escalation: &GateWaitingPayload, beside: Beside) -> Vec<String> {
        let look = Look::plain();
        let doc = Doc::new().with(Block::Lines(lines(
            (&RUN, &NODE),
            escalation,
            &[],
            beside,
            &look,
        )));
        crate::render::surface_lines(doc, &look)
            .iter()
            .map(Line::text)
            .collect()
    }

    #[test]
    fn every_option_has_its_own_command() {
        let commands: Vec<String> = drawn(&escalation(), NOTHING_BESIDE)
            .into_iter()
            .filter(|line| line.contains("yunta resolve-gate"))
            .map(|line| line.trim().to_string())
            .collect();
        assert_eq!(
            commands,
            [
                "yunta resolve-gate T1Y0P5 retry",
                "yunta resolve-gate T1Y0P5 abort"
            ]
        );
    }

    #[test]
    fn an_option_whose_label_is_its_id_is_shown_once() {
        let drawn = drawn(&gate(), NOTHING_BESIDE);
        assert!(
            drawn.iter().any(|line| line.trim() == "approve"),
            "{drawn:#?}"
        );
        assert!(
            !drawn.iter().any(|line| line.contains("approve — approve")),
            "{drawn:#?}"
        );
    }

    #[test]
    fn what_a_page_already_says_beside_a_decision_is_not_said_twice() {
        let all = drawn(&escalation(), NOTHING_BESIDE);
        assert!(all.iter().any(|line| line.contains("are exhausted")));
        assert!(all.iter().any(|line| line.trim() == "exit 1"));
        let page = drawn(
            &escalation(),
            Beside {
                claim: true,
                evidence: true,
            },
        );
        assert!(
            !page
                .iter()
                .any(|line| line.contains("are exhausted") || line.trim() == "exit 1"),
            "{page:#?}"
        );
    }

    #[test]
    fn every_line_of_a_decision_fits_the_line() {
        for line in drawn(&escalation(), NOTHING_BESIDE) {
            assert!(cell_width(&line) <= Look::plain().width.cells(), "{line}");
        }
    }

    #[test]
    fn where_the_run_works_is_said_with_its_path_whole() {
        let line = run_tree_line(Path::new("/home/me/.yunta/worktrees/01J/tree"));
        assert!(
            line.contains("/home/me/.yunta/worktrees/01J/tree"),
            "{line}"
        );
    }

    #[test]
    fn the_json_carries_the_command_for_each_option() {
        let json = serde_json::to_value(DecisionJson::new(&RUN, &NODE, escalation())).unwrap();
        assert_eq!(
            json["commands"]["retry"],
            "yunta resolve-gate 01JBZ5X8K3N7Q2W6E4R9T1Y0P5 retry"
        );
    }

    #[test]
    fn the_json_says_an_engine_claim_in_the_words_a_person_reads() {
        let payload = Escalation::new(
            Claim::NeedsDecision { node: NODE },
            Evidence::none(),
            NonEmpty::from((
                option("approve", "Approve", "lets the gate go on"),
                Vec::new(),
            )),
        )
        .expect("an engine claim repeats no fact")
        .into_payload();
        let json = serde_json::to_value(DecisionJson::new(&RUN, &NODE, payload)).unwrap();
        assert_eq!(json["summary"], "gate `lint` needs a decision");
    }

    #[test]
    fn a_reset_pause_with_no_menu_says_the_run_resumes_on_its_own() {
        let run = RunId::from_static("run-1");
        let lines = without_menu(&run, "node `build` waits for its limit to reset", true, 80);
        let text: String = lines
            .iter()
            .map(|line| format!("{}\n", line.text()))
            .collect();
        assert!(text.contains("resumes on its own"), "{text}");
        assert!(
            !text.contains("yunta resume"),
            "no command is asked of the person: {text}"
        );
    }
}
