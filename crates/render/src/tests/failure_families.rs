//! The words of the failure families a run's gates, loops, children, runners
//! and limits can end a node with. Each sentence is one a reader already reads,
//! pinned so that a change to it is a deliberate one.

use std::path::PathBuf;

use yunta_core::events::{
    Approval, ArtifactId, CapabilityUse, ChildRefusal, Failure, FindingSeverity, Interruption,
    MountError, ObjectProblem, RunnerRefusal, Timed,
};
use yunta_core::inputs::InputsProblem;
use yunta_core::shown::Flaw;
use yunta_core::template::TemplateError;
use yunta_core::{
    AdapterId, AgentName, ArtifactKind, ArtifactRefId, Capability, CommitSha, ContentHash,
    DocumentRef, ExecutorName, FindingId, InputName, JoinPolicy, NodeId, OptionId, Report,
    Responder, RunnerName, SkillName, TaskId,
};

use crate::failure::{detail, headline, sentence};

fn node(name: &str) -> NodeId {
    NodeId::from(name)
}

#[test]
fn a_group_that_failed_under_all_names_its_first_failed_child() {
    let failure = Failure::ChildrenFailed {
        join: JoinPolicy::All,
        children_failed: vec![node("build"), node("test")],
    };
    assert_eq!(sentence(&failure), "child `build` failed under join: all");
}

#[test]
fn a_group_that_failed_under_any_says_how_many_children_failed() {
    let failure = Failure::ChildrenFailed {
        join: JoinPolicy::Any,
        children_failed: vec![node("a"), node("b")],
    };
    assert_eq!(
        sentence(&failure),
        "join: any — no child succeeded (2 failed)"
    );
}

#[test]
fn a_child_left_with_no_terminal_event_names_the_node_and_the_policy_that_stops_it() {
    let failure = Failure::Uncertain {
        uncertain: node("build"),
    };
    assert_eq!(
        sentence(&failure),
        "`build` was running with no terminal event when the engine last stopped — \
         `on_interrupt: fail_if_uncertain` refuses to guess whether it finished; verify manually \
         before resuming"
    );
}

#[test]
fn the_loser_of_a_join_any_race_says_a_sibling_finished_first() {
    let failure = Failure::Interrupted {
        interrupted_by: Interruption::SiblingWon,
    };
    assert_eq!(
        sentence(&failure),
        "interrupted: a sibling in this join: any group finished first"
    );
}

#[test]
fn a_gate_rejected_from_the_console_says_so() {
    let failure = Failure::Rejected {
        rejected_by: Responder::from_static("ana"),
        on: Approval::Console,
    };
    assert_eq!(sentence(&failure), "rejected from the console");
}

#[test]
fn a_plan_sent_back_names_its_first_flaw_and_how_many_more_there_are() {
    let flaw = |task: &str| Flaw::PassesByAName {
        task: TaskId::from(task),
        cmd: "cargo test".to_string(),
        file: "src/lib.rs".to_string(),
    };
    let one = Failure::SentBack {
        sent_back: vec![flaw("t1")],
    };
    assert_eq!(
        sentence(&one),
        "not published, and sent back: the plan cannot be proven as it is written: `t1` is \
         judged by `cargo test`, which passes once a name is written in `src/lib.rs`"
    );
    let three = Failure::SentBack {
        sent_back: vec![flaw("t1"), flaw("t2"), flaw("t3")],
    };
    assert!(
        sentence(&three).ends_with("in `src/lib.rs` (and 2 more)"),
        "{}",
        sentence(&three)
    );
}

#[test]
fn open_findings_name_their_count_their_severity_and_their_ids() {
    let failure = Failure::FindingsOpen {
        at_or_above: FindingSeverity::Major,
        open_findings: vec![
            FindingId::from_static("review-0"),
            FindingId::from_static("review-1"),
        ],
    };
    assert_eq!(
        sentence(&failure),
        "2 findings at or above major: review-0, review-1"
    );
}

#[test]
fn changes_requested_name_the_reviewer_the_commit_and_the_comments() {
    let failure = Failure::ChangesRequested {
        requested_by: "ana".to_string(),
        reviewed_sha: CommitSha::from("abc1234"),
        comments: 2,
    };
    assert_eq!(
        sentence(&failure),
        "changes requested by ana at abc1234 (2 comments)"
    );
}

#[test]
fn a_corrupt_object_names_the_hash_asked_for_and_the_one_it_holds() {
    let hash = yunta_core::sha256_hex(b"accepted");
    let found = yunta_core::sha256_hex(b"tampered");
    let text = crate::failure::object(&ObjectProblem::Corrupt {
        hash: hash.clone(),
        found: found.clone(),
    });
    assert!(
        text.contains(hash.as_str()),
        "names the hash asked for: {text}"
    );
    assert!(text.contains(found.as_str()), "names what it found: {text}");
}

#[test]
fn a_pull_request_closed_without_approval_says_so_whether_or_not_it_is_named() {
    let unknown = Failure::PullRequestClosed {
        closed_pull_request: None,
    };
    assert_eq!(
        sentence(&unknown),
        "the pull request was closed without approval"
    );
    let known = Failure::PullRequestClosed {
        closed_pull_request: Some("https://forge/pr/1".to_string()),
    };
    assert!(matches!(
        &known,
        Failure::PullRequestClosed {
            closed_pull_request: Some(reference),
        } if reference == "https://forge/pr/1"
    ));
    assert_eq!(
        sentence(&known),
        "the pull request was closed without approval"
    );
}

#[test]
fn a_gate_choice_names_the_option_the_node_it_routes_to_and_what_was_written() {
    let bare = Failure::GateChose {
        node: yunta_core::NodeId::from("gate"),
        option: OptionId::from_static("retry"),
        target: node("build"),
        said: None,
    };
    assert_eq!(
        sentence(&bare),
        "gate chose `retry` — re-routing to `build`"
    );
    let written = Failure::GateChose {
        node: yunta_core::NodeId::from("gate"),
        option: OptionId::from_static("retry"),
        target: node("build"),
        said: Some("try the other fixture".to_string()),
    };
    assert_eq!(
        sentence(&written),
        "gate chose `retry` — re-routing to `build`: try the other fixture"
    );
    assert_eq!(
        headline(&written),
        "gate chose `retry` — re-routing to `build`"
    );
    assert_eq!(detail(&written), vec!["try the other fixture"]);
}

#[test]
fn a_stalled_loop_and_a_loop_without_a_tasks_document_say_what_they_need() {
    assert_eq!(
        sentence(&Failure::LoopStalled {
            not_ready: vec![TaskId::from("t1")],
        }),
        "no task is ready and not all are done — blocked or failed tasks need a decision"
    );
    assert_eq!(
        sentence(&Failure::NoTasksDocument {
            tasks_for: node("loop"),
        }),
        "this run holds no tasks document — an earlier node declares `produces: [tasks]`, or the \
         workflow declares a `type: document` input of `kind: tasks` and the run is given one"
    );
}

#[test]
fn a_loop_at_its_iteration_cap_names_the_limit_and_the_two_ways_past_it() {
    assert_eq!(
        sentence(&Failure::LoopCapped {
            node: yunta_core::NodeId::from("loop"),
            loop_cap: 3
        }),
        "loop `loop` exceeded `limits.max_loop_iterations` (3) — resume with an interactive surface \
         to continue past the cap or abort"
    );
}

#[test]
fn a_runner_that_does_not_resolve_says_why_it_did_not() {
    let unknown = Failure::RunnerUnresolved {
        runner: RunnerName::from("fast"),
        runner_refusal: RunnerRefusal::Unknown,
    };
    assert_eq!(
        sentence(&unknown),
        "no `runners:` entry defines runner `fast` — add it to the merged config"
    );
    let none = Failure::RunnerUnresolved {
        runner: RunnerName::from("fast"),
        runner_refusal: RunnerRefusal::NoCandidate {
            tried: vec![
                AdapterId::from_static("mock"),
                AdapterId::from_static("claude"),
            ],
        },
    };
    assert_eq!(
        sentence(&none),
        "runner `fast` has no available candidate — tried adapters: mock, claude"
    );
}

#[test]
fn a_missing_capability_says_what_the_node_needs_it_for() {
    let missing = |used_for: Option<CapabilityUse>, agent: Option<AgentName>| {
        sentence(&Failure::CapabilityMissing {
            node: yunta_core::NodeId::from("implement"),
            capability: Capability::RunTools,
            adapter: AdapterId::from_static("mock"),
            used_for,
            agent,
        })
    };
    assert!(missing(Some(CapabilityUse::Blackboard), None).starts_with(
        "node `implement` is in a `coordination: blackboard` group but adapter `mock` declares no \
             `run_tools` capability — the blackboard cannot be mounted"
    ));
    assert!(missing(
        Some(CapabilityUse::TypedArtifact {
            kind: ArtifactKind::Tasks
        }),
        None
    )
    .starts_with("node `implement` declares a `tasks` artifact, which a session hands over"));
    assert!(missing(Some(CapabilityUse::LoopTasks), None)
        .starts_with("node `implement` is a loop, whose task sessions read their task"));
    assert_eq!(
        missing(None, Some(AgentName::from("reviewer"))),
        "a node requests agent `reviewer` but adapter `mock` does not declare `custom_agents` — \
         pick a candidate on an adapter that does, or drop the agent"
    );
}

#[test]
fn a_session_or_an_executor_past_its_bound_says_which_bound_it_passed() {
    assert_eq!(
        sentence(&Failure::TimedOut {
            after_ms: 1000,
            timed: Timed::Session,
        }),
        "exceeded timeout of 1s"
    );
    assert_eq!(
        sentence(&Failure::TimedOut {
            after_ms: 2000,
            timed: Timed::Executor {
                executor: ExecutorName::from("lint"),
            },
        }),
        "executor `lint` exceeded its 2s timeout"
    );
    assert_eq!(
        sentence(&Failure::TokenBudget {
            used: 150,
            max: 100,
        }),
        "exceeded max_tokens 100 (150 used)"
    );
}

#[test]
fn a_child_nested_too_deep_names_itself_the_level_it_reaches_and_the_limit() {
    assert_eq!(
        sentence(&Failure::NestedTooDeep {
            workflow: "sub".to_string(),
            depth: 5,
            max_depth: 4,
        }),
        "creating child workflow `sub` would nest 5 levels deep but `limits.max_workflow_depth` \
         is 4 — flatten the composition or raise the limit"
    );
}

#[test]
fn a_template_that_does_not_render_under_a_child_input_names_the_input() {
    let failure = Failure::Template {
        template: TemplateError::Unclosed { at: 4 },
        input: Some(InputName::from("goal")),
    };
    assert_eq!(
        sentence(&failure),
        "child input `goal` does not render: unclosed `{{` at byte 4 — every template must \
         close with `}}`"
    );
}

#[test]
fn a_child_whose_inputs_do_not_fit_says_the_input_it_refused() {
    let failure = Failure::ChildRefused {
        workflow: "sub".to_string(),
        refusal: Box::new(ChildRefusal::Inputs {
            problem: InputsProblem::Missing {
                name: InputName::from("goal"),
            },
        }),
    };
    assert_eq!(
        sentence(&failure),
        "input `goal` is required and has no default"
    );
}

#[test]
fn a_child_that_fails_its_check_names_the_use_and_the_rules_it_refused() {
    let failure = Failure::ChildRefused {
        workflow: "sub".to_string(),
        refusal: Box::new(ChildRefusal::FailsCheck {
            rules: vec!["composition_fails".to_string()],
        }),
    };
    assert_eq!(
        headline(&failure),
        "`use: sub` fails check: composition_fails"
    );
}

#[test]
fn a_skill_the_run_does_not_hold_names_the_roots_it_searched() {
    let failure = Failure::SkillMissing {
        missing_skill: SkillName::from_static("review"),
        searched: vec![PathBuf::from(".yunta/skills"), PathBuf::from("packs/x")],
    };
    assert_eq!(
        sentence(&failure),
        "skill `review` not found under `.yunta/skills`, `packs/x` — add the directory, fix \
         `skills.paths` in the config, or install the pack that declares it"
    );
}

#[test]
fn an_artifact_the_run_cannot_read_says_the_object_it_lacks() {
    let hash = ContentHash::sha256(b"the accepted bytes");
    let failure = Failure::ArtifactUnreadable {
        artifact: ArtifactId::Interpreted {
            kind: ArtifactKind::Tasks,
        },
        problem: ObjectProblem::Missing { hash: hash.clone() },
    };
    assert_eq!(
        sentence(&failure),
        format!(
            "the tasks document: the child run cannot hand over its bytes: the run holds no \
             object `{hash}` — an artifact names bytes that are not under `objects/`"
        )
    );
}

#[test]
fn an_external_gate_that_publishes_what_no_node_made_names_the_artifact() {
    let failure = Failure::GateArtifactMissing {
        node: yunta_core::NodeId::from("gate"),
        gate_artifact: ArtifactId::Opaque {
            name: "report.md".to_string(),
        },
    };
    assert_eq!(
        sentence(&failure),
        "node `gate` publishes the artifact `report.md` with its external gate, and this run holds \
         no such artifact — no node produced it"
    );
}

#[test]
fn a_mount_whose_child_run_reached_no_terminal_state_names_the_artifact_and_the_node() {
    let failure = Failure::MountUnreachable {
        error: Box::new(MountError::NoTerminalChild {
            artifact: ArtifactRefId::Name {
                name: "report.md".to_string(),
            },
            node: node("a"),
        }),
    };
    assert_eq!(
        sentence(&failure),
        "mount of the report.md from node `a`: no linked child run of `a` reached a terminal \
         state in this run — did this run's mode exclude it?"
    );
}

#[test]
fn a_rejected_inputs_document_names_the_input_and_what_it_must_be() {
    let problem = InputsProblem::DocumentRefused {
        name: InputName::from("plan"),
        report: Report::new(
            DocumentRef::new(ArtifactKind::Tasks, "artifacts/plan.yaml"),
            Vec::new(),
        ),
    };
    assert!(
        crate::inputs::problem(&problem)
            .starts_with("input `plan` is not a tasks document this run can hold\n"),
        "{}",
        crate::inputs::problem(&problem)
    );
}
