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

use surco::components::{Decision, DecisionOption, Gist, Note, Prose, Section};
use surco::doc::Block;
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use yunta_core::events::GateWaitingPayload;
use yunta_core::RunId;
use yunta_render::escalation;

use crate::commands::advice;
use crate::run_page::Decided;

/// Where a run works, as the page says it above a decision: the path
/// whole — it is copied into another terminal, never cut — and what it
/// means for a decision about the run.
pub(crate) fn run_tree_line(tree: &Path) -> String {
    surco::text::detailed(
        "the run works in",
        &format!(
            "{} (a node run again starts from what is there)",
            tree.display()
        ),
    )
}

/// What the surface showing a decision says beside it already.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Beside {
    /// The claim the escalation makes — a page that asks with the run's
    /// pause already said it.
    pub(crate) claim: bool,
    /// The record the engine attached — a page that quotes a failed
    /// node's evidence already shows it.
    pub(crate) evidence: bool,
}

/// The decision a parked run waits on, as the section a page ends on:
/// what the decision says happened and what it is about — less what the
/// page says beside it — what weighs on it, every option with the command
/// that chooses it, and `otherwise`: what moves the run without choosing
/// one.
pub(crate) fn section(
    run_id: &RunId,
    decided: &Decided,
    otherwise: Vec<(String, &'static str)>,
) -> Section {
    let escalation = &decided.escalation;
    let mut blocks = account(escalation, decided.beside);
    for shown in escalation.shows() {
        let about = format!("about: {}", yunta_engine::view_of(shown).display());
        blocks.push(Prose(about).into());
    }
    if let Some(external_ref) = escalation.external_ref() {
        blocks.push(Prose(format!("published at {external_ref}")).into());
    }
    if let Some(before) =
        yunta_render::shown::before_you_decide(escalation.withheld(), &decided.shown)
    {
        blocks.push(before.into());
    }
    let mut menu = menu(run_id, escalation);
    menu.otherwise = otherwise;
    blocks.push(menu.into());
    Section {
        lead: Some(surco::components::Lead::Mark(Mark::NeedsYou)),
        title: Line::new().push(Tone::Strong, format!("decision on node `{}`", decided.node)),
        blocks,
    }
}

/// What a decision says happened, less what the surface showing it says
/// beside it: the claim, and the record the engine attached to audit it
/// against.
pub(crate) fn account(escalation: &GateWaitingPayload, beside: Beside) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    if !beside.claim {
        blocks.push(Gist::new(claim_words(escalation)).into());
    }
    if !beside.evidence {
        blocks.extend(
            escalation::evidence(escalation)
                .into_iter()
                .map(|fact| Note(fact).into()),
        );
    }
    blocks
}

/// The claim of a decision in the words a reader sees: what the escalation
/// says happened, with the re-route a failed node has used up named in it.
pub(crate) fn claim_words(escalation: &GateWaitingPayload) -> String {
    yunta_render::claim::sentence(escalation.summary(), escalation.failure())
}

/// Every option of a decision: what it costs and the command that
/// chooses it.
fn menu(run_id: &RunId, escalation: &GateWaitingPayload) -> Decision {
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
    yunta_render::blocks::decision::here(run_id, options, &advice::spell)
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        AuthorGate, Claim, Escalation, ExhaustedReroute, Failure, FailureMenu, GateOption, Menu,
    };
    use yunta_core::reserved::ReservedOption;
    use yunta_core::NonEmpty;

    use super::*;
    use surco::doc::Doc;
    use surco::look::Look;
    use surco::width::cell_width;
    use yunta_core::NodeId;

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
                node: None,
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
        let decided = Decided {
            node: NODE,
            escalation: escalation.clone(),
            shown: Vec::new(),
            beside,
        };
        let close = (advice::close(RUN.handle()), "closes it for good");
        let doc = Doc::new().with(section(&RUN, &decided, vec![close]));
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
            !drawn.iter().any(|line| line.contains("approve: approve")),
            "{drawn:#?}"
        );
    }

    #[test]
    fn what_a_page_already_says_beside_a_decision_is_not_said_twice() {
        // The record is said under the claim, a fact to a line.
        let quoted = |line: &String| line.trim() == "exit 1";
        let all = drawn(&escalation(), NOTHING_BESIDE);
        assert!(all.iter().any(|line| line.contains("are exhausted")));
        assert!(all.iter().any(quoted), "{all:#?}");
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
                .any(|line| line.contains("are exhausted") || quoted(line)),
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
    fn what_moves_the_run_without_an_option_comes_after_every_option() {
        let drawn = drawn(&escalation(), NOTHING_BESIDE);
        let or = drawn.iter().position(|line| line.trim() == "or");
        let abort = drawn
            .iter()
            .position(|line| line.trim() == "yunta resolve-gate T1Y0P5 abort");
        assert!(or > abort && abort.is_some(), "{drawn:#?}");
        assert!(
            drawn
                .last()
                .is_some_and(|line| line.contains("yunta close T1Y0P5")),
            "{drawn:#?}"
        );
    }
}
