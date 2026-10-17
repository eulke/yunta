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

use std::path::Path;

use yunta_core::events::GateWaitingPayload;
use yunta_core::{NodeId, RunId};
use yunta_engine::ShownDocument;

use crate::commands::advice;
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
pub(crate) fn claim_words(escalation: &GateWaitingPayload) -> String {
    yunta_render::claim::sentence(escalation.summary(), escalation.failure())
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
            let (label, tradeoff) = yunta_render::offers::option_words(escalation, option);
            DecisionOption {
                id: option.id.to_string(),
                label: Some(label),
                tradeoff,
                asks: yunta_render::offers::asks(option),
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

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        AuthorGate, Claim, Escalation, ExhaustedReroute, Failure, FailureMenu, GateOption, Menu,
    };
    use yunta_core::reserved::ReservedOption;
    use yunta_core::NonEmpty;

    use super::*;
    use crate::render::doc::{Block, Doc};

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const NODE: NodeId = NodeId::from_static("lint");

    /// A failed node whose re-route to `fix-lint` is used up: its menu offers
    /// the retry through `fix-lint` once more, and the abort every menu ends with.
    fn escalation() -> GateWaitingPayload {
        Escalation::new(
            Claim::NodeFailed { node: NODE },
            Menu::Failure(Box::new(FailureMenu {
                attempt: 1,
                failure: Failure::Message {
                    outcome: "exit 1".into(),
                },
                remedies: Vec::new(),
                stuck: None,
                reroute: Some(ExhaustedReroute {
                    goto: NodeId::from_static("fix-lint"),
                    max_reroutes: 0,
                    next_mode: None,
                }),
            })),
            NonEmpty::from((
                GateOption::reserved(ReservedOption::Retry),
                vec![GateOption::reserved(ReservedOption::Abort)],
            )),
        )
        .into_payload()
    }

    /// A gate an author declared, whose one option is labelled with its own
    /// id: no words name the reserved approve on this menu.
    fn gate() -> GateWaitingPayload {
        Escalation::new(
            Claim::NeedsDecision { node: NODE },
            Menu::Gate(AuthorGate {
                assignee: "lead".to_string(),
            }),
            NonEmpty::from((GateOption::reserved(ReservedOption::Approve), Vec::new())),
        )
        .into_payload()
    }

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
