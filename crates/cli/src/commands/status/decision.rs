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
//! run leaves on the terminal show the same decision the same way: the
//! record the engine attached to it, what it is about, what weighs on it,
//! every option with the tradeoff that makes it a choice, and the command
//! that chooses it. A failed node's evidence is said once: by the block,
//! or by the page above it when the page quotes it.
//!
//! **Three pauses reconstruct a menu, and only three**: a failed node,
//! whose reroutes are exhausted or that has none, an unresolved internal
//! gate, and a run stopped on its token cap, whose decision is the run's and
//! names no node. A scope expansion in ask mode, an unanswered questions
//! artifact, a parent waiting on its child and an external gate with no
//! reachable forge park a run with no menu to rebuild — the forge one because whether a forge answers depends
//! on the machine asking, which no log can say. Those runs still report
//! what they are waiting on, and what to do instead of choosing an
//! option.

use std::path::Path;

use surco::components::{Decision, Note, Prose, Section};
use surco::doc::Block;
use yunta_core::events::{DecisionOn, GateWaitingPayload};
use yunta_core::shown::ShownDocument;
use yunta_core::RunId;

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
    /// The record the engine attached — a page that quotes a failed
    /// node's evidence already shows it.
    pub(crate) evidence: bool,
}

/// The decision a parked run waits on, as the section a page ends on: the
/// record the engine attached to it and what it is about, less what the
/// page says beside it. Then what weighs on it, every option with the
/// command that chooses it, and `otherwise`: what moves the run without
/// choosing one.
pub(crate) fn section(
    run_id: &RunId,
    decided: &Decided,
    otherwise: Vec<(String, &'static str)>,
) -> Section {
    let escalation = &decided.escalation;
    let mut blocks = account(escalation, decided.beside, &decided.shown);
    for shown in escalation.shows() {
        let about = format!("about: {}", yunta_core::run_dir::view_of(shown).display());
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
    let heading = match &decided.on {
        DecisionOn::Node(node) => format!("decision on node `{node}`"),
        DecisionOn::Run => "decision on the run".to_string(),
    };
    yunta_render::gate::decision(heading, blocks)
}

/// The record the engine attached to a decision, to audit it against, less
/// what the surface showing it says beside it: a page that quotes a failed
/// node's evidence draws none of it again. Which reasons the record leaves
/// to "before you decide" is decided by `gate::record`, from the plans on
/// the page, so the status page and the prompt say each reason once.
fn account(escalation: &GateWaitingPayload, beside: Beside, shown: &[ShownDocument]) -> Vec<Block> {
    match beside.evidence {
        true => Vec::new(),
        false => yunta_render::gate::record(escalation, shown)
            .into_iter()
            .map(|fact| Note(fact).into())
            .collect(),
    }
}

/// The claim of a decision in the words a reader sees: what the escalation
/// says happened, with the reroute a failed node has used up named in it.
pub(crate) fn claim_words(escalation: &GateWaitingPayload) -> String {
    yunta_render::claim::sentence(escalation.summary(), escalation.failure())
}

/// Every option of a decision: what it costs and the command that
/// chooses it.
fn menu(run_id: &RunId, escalation: &GateWaitingPayload) -> Decision {
    let options = yunta_render::offers::decision_options(escalation);
    yunta_render::blocks::decision::here(run_id, options)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use yunta_core::events::{
        ArtifactId, AuthorGate, Claim, Escalation, ExhaustedReroute, Failure, FailureMenu,
        GateOption, Menu, Shown, WhyWithheld, Withheld,
    };
    use yunta_core::reserved::ReservedOption;
    use yunta_core::shown::{
        DeniedChange, Flaw, PlanReview, ShownContent, ShownDocument, TaskReview,
    };
    use yunta_core::NonEmpty;

    use super::*;
    use surco::doc::Doc;
    use surco::line::Line;
    use surco::look::Look;
    use surco::width::cell_width;
    use yunta_core::step::NextStep;
    use yunta_core::{ArtifactKind, ContentHash, NodeId, TaskId, TasksFile};

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const NODE: NodeId = NodeId::from_static("lint");

    /// A failed node whose reroute to `fix-lint` is used up: its menu offers
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

    const NOTHING_BESIDE: Beside = Beside { evidence: false };

    /// The lines of the section a page ends on for `escalation`, with nothing shown.
    fn drawn(escalation: &GateWaitingPayload, beside: Beside) -> Vec<String> {
        drawn_with(&Decided {
            on: DecisionOn::Node(NODE),
            escalation: escalation.clone(),
            shown: Vec::new(),
            beside,
        })
    }

    /// A decision no node owns says whose it is in its heading.
    #[test]
    fn a_decision_the_run_owns_says_so() {
        let lines = drawn_with(&Decided {
            on: DecisionOn::Run,
            escalation: escalation(),
            shown: Vec::new(),
            beside: NOTHING_BESIDE,
        });
        assert!(
            lines
                .iter()
                .any(|line| line.contains("decision on the run")),
            "{lines:#?}"
        );
        assert!(
            !lines.iter().any(|line| line.contains("decision on node")),
            "{lines:#?}"
        );
    }

    /// The lines of the section a page ends on for `decided`, drawn plain.
    fn drawn_with(decided: &Decided) -> Vec<String> {
        let close = (
            yunta_render::command::line(&NextStep::Close { run: RUN }),
            "closes it for good",
        );
        let doc = Doc::new().with(section(&RUN, decided, vec![close]));
        crate::render::surface_lines(doc, &Look::plain())
            .iter()
            .map(Line::text)
            .collect()
    }

    /// A gate that withholds `continue` for a flaw of the plan it is about.
    fn withholding_for_a_flaw() -> GateWaitingPayload {
        let flaw = Flaw::HollowSpecTest {
            task: TaskId::from("T001"),
            cmd: "test -f a.txt".to_string(),
        };
        Escalation::new(
            Claim::NeedsDecision { node: NODE },
            Menu::Gate(AuthorGate {
                assignee: "lead".to_string(),
                node: None,
            }),
            NonEmpty::from((
                GateOption::reserved(ReservedOption::Abort),
                vec![GateOption::reserved(ReservedOption::Continue)],
            )),
        )
        .withholding(vec![Withheld {
            option: "continue".into(),
            because: WhyWithheld::Unprovable {
                flaws: NonEmpty::from((flaw, Vec::new())),
            },
        }])
        .into_payload()
    }

    /// A plan of one task whose change is on a test another task's spec wrote,
    /// so the plan cannot be proven as it is written.
    fn flawed_plan() -> ShownDocument {
        let plan: TasksFile = yunta_core::yaml::parse(
            "tasks:\n  - id: T001\n    title: Make it\n    scope: [a.txt]\n    criteria: []\n",
        )
        .expect("the plan reads");
        let review = PlanReview {
            plan,
            departed: Vec::new(),
            spec: None,
            spec_node: None,
            asked: None,
            held: Vec::new(),
            tasks: vec![TaskReview {
                task: TaskId::from("T001"),
                criteria: Vec::new(),
                denied: vec![DeniedChange {
                    at: "tests/a.rs".to_string(),
                    owner: TaskId::from("T002"),
                }],
                guards: Vec::new(),
            }],
            handed_over: None,
            answered: Vec::new(),
            shapes_read: None,
        };
        ShownDocument {
            shown: Shown {
                producer: Some("plan".into()),
                artifact: ArtifactId::Interpreted {
                    kind: ArtifactKind::Tasks,
                },
                content_hash: ContentHash::sha256(b"plan"),
            },
            path: PathBuf::from("artifacts/plan/tasks.yaml"),
            content: ShownContent::Tasks(Box::new(review)),
        }
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
                "|   yunta resolve-gate T1Y0P5 retry",
                "|   yunta resolve-gate T1Y0P5 abort"
            ]
        );
    }

    #[test]
    fn an_option_whose_label_is_its_id_is_shown_once() {
        let drawn = drawn(&gate(), NOTHING_BESIDE);
        assert!(
            drawn.iter().any(|line| line.trim() == "| approve"),
            "{drawn:#?}"
        );
        assert!(
            !drawn.iter().any(|line| line.contains("approve: approve")),
            "{drawn:#?}"
        );
    }

    #[test]
    fn what_a_page_already_says_beside_a_decision_is_not_said_twice() {
        // The record is said as a fact to a line; the page above the decision
        // says its claim.
        let quoted = |line: &String| line.trim() == "exit 1";
        let all = drawn(&escalation(), NOTHING_BESIDE);
        assert!(all.iter().any(quoted), "{all:#?}");
        assert!(
            !all.iter().any(|line| line.contains("are exhausted")),
            "{all:#?}"
        );
        let page = drawn(&escalation(), Beside { evidence: true });
        assert!(!page.iter().any(quoted), "{page:#?}");
    }

    /// The lines of a page read as one text, each run of spaces as one space:
    /// a sentence a terminal broke over lines reads whole.
    fn as_text(lines: &[String]) -> String {
        lines
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// With a plan on the page that says why an option is withheld, the record
    /// leaves the reason out, and the section says it once, under its title.
    #[test]
    fn a_withheld_reason_is_said_once_by_before_you_decide() {
        let withheld = withholding_for_a_flaw();
        let record = account(&withheld, NOTHING_BESIDE, &[flawed_plan()]);
        let page = Doc::new().with_all(record);
        let drawn: Vec<String> = crate::render::surface_lines(page, &Look::plain())
            .iter()
            .map(Line::text)
            .collect();
        assert!(
            drawn.iter().any(|line| line.contains("assignee: lead")),
            "{drawn:#?}"
        );
        assert!(
            !drawn.iter().any(|line| line.contains("withheld")),
            "{drawn:#?}"
        );
        // The section a page ends on says the reason once, under its title.
        let decided = Decided {
            on: DecisionOn::Node(NODE),
            escalation: withheld,
            shown: vec![flawed_plan()],
            beside: NOTHING_BESIDE,
        };
        let lines = drawn_with(&decided);
        let reason = "is not offered, because";
        let title = lines
            .iter()
            .position(|line| line.contains("before you decide"));
        let said = lines.iter().position(|line| line.contains(reason));
        assert!(title.is_some() && title < said, "{lines:#?}");
        assert_eq!(
            lines.iter().filter(|line| line.contains(reason)).count(),
            1,
            "{lines:#?}"
        );
    }

    /// With no plan on the page nothing else says why an option is withheld,
    /// so the record names the option and the reason, whole.
    #[test]
    fn a_withheld_reason_is_said_by_the_record_when_no_plan_is_shown() {
        let lines = drawn(&withholding_for_a_flaw(), NOTHING_BESIDE);
        let reason = "withheld: `continue`, because the plan cannot be proven as it is \
                      written: the spec's test of `T001`, `test -f a.txt`, runs none of \
                      the files the spec wrote";
        assert!(as_text(&lines).contains(reason), "{lines:#?}");
        assert!(
            !lines.iter().any(|line| line.contains("before you decide")),
            "{lines:#?}"
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
        let or = drawn.iter().position(|line| line.trim() == "| or");
        let abort = drawn
            .iter()
            .position(|line| line.trim() == "|   yunta resolve-gate T1Y0P5 abort");
        assert!(or > abort && abort.is_some(), "{drawn:#?}");
        assert!(
            drawn
                .last()
                .is_some_and(|line| line.contains("yunta close T1Y0P5")),
            "{drawn:#?}"
        );
    }
}
