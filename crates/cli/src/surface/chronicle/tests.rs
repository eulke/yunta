use yunta_core::events::{
    BaselineCapturedPayload, BaselineOrigin, BaselineResults, CapabilityDegradedPayload,
    CriteriaCheckedPayload, CriterionResult, EventBody, EventPayload, Evidence, Failure, Finding,
    FindingPostedPayload, FindingSeverity, NodeEvent, NodeFinishedPayload, NodeOutcome,
    NodeReroutedPayload, Phase, PromotionCause, PromotionSignaledPayload, RerouteOrigin, RunEvent,
    SessionEvent, StoredEvent,
};
use yunta_core::events::{FindingEvent, TokenUsage};
use yunta_engine::chronicle as derive_chronicle;

use super::*;

/// What a surface says for one event, through the one derivation
/// every surface reads: the log, folded, then the words.
fn said_for(payload: EventPayload) -> String {
    let events = vec![StoredEvent {
        seq: 1.into(),
        run_id: "01JQ0000000000000000000000".into(),
        node_id: None,
        timestamp: chrono::DateTime::UNIX_EPOCH,
        body: EventBody::Known(payload),
    }];
    let moments = derive_chronicle(&events);
    say(
        moments.first().expect("one moment per event"),
        Glyphs::Unicode,
    )
    .text
}

/// A log this binary reads back was written by some other
/// invocation: nothing guarantees the free text on a payload says
/// anything, and words built for it must not promise that it does.
fn rerouted(cause: &str) -> EventPayload {
    EventPayload::Node(NodeEvent::Rerouted(NodeReroutedPayload::new(
        "fix-lint".into(),
        Failure::message(cause),
        RerouteOrigin::GateChoice,
        None,
        None,
    )))
}

/// A child that failed or was cancelled is not marked as one that
/// finished: the mark repeats how it closed.
#[test]
fn a_closed_child_is_marked_by_how_it_closed() {
    use yunta_core::events::{ChildEvent, ChildRunFinishedPayload, TerminalState};
    for (terminal, mark) in [
        (TerminalState::Done, Mark::Done),
        (TerminalState::Failed, Mark::Failed),
        (TerminalState::Cancelled, Mark::Failed),
        (TerminalState::Promoted, Mark::Reroute),
    ] {
        let events = vec![StoredEvent {
            seq: 1.into(),
            run_id: "01JQ0000000000000000000000".into(),
            node_id: Some("compose".into()),
            timestamp: chrono::DateTime::UNIX_EPOCH,
            body: EventBody::Known(EventPayload::Children(ChildEvent::Finished(
                ChildRunFinishedPayload {
                    child_run_id: "01JQ0000000000000000000001".into(),
                    child_workflow_hash: yunta_core::sha256_hex(b"child"),
                    terminal_state: terminal,
                    tokens: TokenUsage::default(),
                },
            ))),
        }];
        let moments = derive_chronicle(&events);
        let said = say(moments.first().expect("one moment"), Glyphs::Unicode);
        assert_eq!(said.mark, Some(mark), "{terminal:?}: {}", said.text);
    }
}

#[test]
fn a_reroute_with_a_cause_reads_as_the_target_and_the_cause() {
    assert_eq!(
        said_for(rerouted("exit 1")),
        "run — rerouted to `fix-lint`: exit 1"
    );
}

#[test]
fn a_red_post_check_names_each_red_criterion_with_the_last_line_it_printed() {
    let result = |cmd: &str, exit_code: i32, tail: &[&str]| CriterionResult {
        cmd: cmd.to_string(),
        exit_code,
        r#type: None,
        reused: false,
        duration_ms: None,
        output: None,
        tail: tail.iter().map(|line| line.to_string()).collect(),
        tree: None,
        head: None,
        conclusion: None,
    };
    let payload = EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
        task_id: "T001".into(),
        phase: Phase::Post,
        results: vec![
            result("cargo test", 101, &["failures:", "test result: FAILED", ""]),
            result("cargo fmt --check", 0, &[]),
            result("test -f made.txt", 1, &[]),
        ],
        waiting: Vec::new(),
    }));
    assert_eq!(
        said_for(payload),
        "run — T001 post: 3 criteria · red: `cargo test` exit 101 — test result: FAILED; \
         `test -f made.txt` exit 1"
    );
}

#[test]
fn a_check_answer_says_how_long_it_took_and_what_stands_between_the_task_and_done() {
    let answered = |closes: bool, exit_code: i32, outside: &[&str]| {
        EventPayload::Tasks(yunta_core::events::TaskEvent::CheckAnswered(
            yunta_core::events::TaskCheckAnsweredPayload {
                task_id: "T001".into(),
                closes,
                results: vec![CriterionResult {
                    cmd: "cargo test".to_string(),
                    exit_code,
                    r#type: None,
                    reused: false,
                    duration_ms: Some(221_000),
                    output: None,
                    tail: Vec::new(),
                    tree: None,
                    head: None,
                    conclusion: None,
                }],
                outside_scope: outside.iter().map(|path| path.into()).collect(),
                denied: Vec::new(),
                duration_ms: 222_400,
                waiting: Vec::new(),
            },
        ))
    };
    assert_eq!(
        said_for(answered(true, 0, &[])),
        "run — T001 checked in 3m42s: it would close"
    );
    assert_eq!(
        said_for(answered(false, 101, &[])),
        "run — T001 checked in 3m42s: 1 of its criteria red"
    );
    assert_eq!(
        said_for(answered(false, 0, &["notes.md"])),
        "run — T001 checked in 3m42s: its work reaches paths it may not change"
    );
}

#[test]
fn a_reroute_with_no_cause_recorded_reads_as_the_target_alone() {
    assert_eq!(said_for(rerouted("")), "run — rerouted to `fix-lint`");
}

#[test]
fn a_promotion_names_the_node_whose_reroutes_ran_out() {
    let cause = PromotionCause {
        node: "lint".into(),
        goto: "fix-lint".into(),
        failure: None,
    };
    let promoted = PromotionSignaledPayload::new(cause, Evidence::none(), "ship".into());
    assert_eq!(
        said_for(EventPayload::Run(RunEvent::PromotionSignaled(promoted))),
        "run — promotion to `ship`: node `lint` exhausted its re-routes to `fix-lint`"
    );
}

fn baseline(exit_code: i32) -> EventPayload {
    EventPayload::Run(RunEvent::BaselineCaptured(BaselineCapturedPayload {
        command: "make test".to_string(),
        results: BaselineResults {
            exit_code,
            summary: String::new(),
        },
        hash: yunta_core::sha256_hex(b""),
        origin: BaselineOrigin::Measured,
        tree: None,
        duration_ms: None,
    }))
}

/// A person watching a run learns the machine slept, that the time is
/// not counted as work, and why no session opens for a while, above
/// the region that keeps moving.
#[test]
fn a_host_suspension_stays_above_the_live_region_and_says_how_long() {
    let payload = EventPayload::Run(RunEvent::HostSuspended(
        yunta_core::events::HostSuspendedPayload::slept(std::time::Duration::from_secs(38 * 60)),
    ));
    assert_eq!(
        said_for(payload.clone()),
        "run — host suspended for 38m00s — durations leave it out; new sessions wait \
         until it has been awake 2m00s"
    );
    let events = vec![StoredEvent {
        seq: 1.into(),
        run_id: "01JQ0000000000000000000000".into(),
        node_id: None,
        timestamp: chrono::DateTime::UNIX_EPOCH,
        body: EventBody::Known(payload),
    }];
    assert!(kept(&derive_chronicle(&events)[0].happening));
}

#[test]
fn a_baseline_measured_green_reads_as_measured() {
    assert_eq!(said_for(baseline(0)), "run — baseline measured");
}

/// A person watching learns, before any comparison passes, that none
/// could have failed.
#[test]
fn a_baseline_measured_red_says_no_comparison_can_find_a_regression() {
    assert_eq!(
        said_for(baseline(101)),
        "run — baseline measured, already red (exit 101): no comparison can find a \
         regression, and no task is held to it"
    );
}

#[test]
fn a_finding_with_no_title_reads_as_its_severity_alone() {
    let payload = EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
        finding: Finding {
            id: "f1".into(),
            severity: FindingSeverity::Minor,
            title: String::new(),
            location: "src/lib.rs".into(),
            detail: String::new(),
            proposed_criterion: None,
        },
    }));
    assert_eq!(said_for(payload), "run — finding f1 minor");
}

#[test]
fn a_node_that_finished_saying_nothing_is_still_said_to_have_finished() {
    let payload = EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
        NodeOutcome::Summary(String::new()),
        TokenUsage::default(),
    )));
    assert_eq!(said_for(payload), "run — finished");
}

/// A degradation whose recorded policy is an empty sentence, as an
/// older log stores it, says what was missing and claims nothing
/// about what was done instead.
#[test]
fn a_degraded_capability_with_no_policy_recorded_reads_as_what_was_missing() {
    let degraded = CapabilityDegradedPayload {
        capability: yunta_core::Capability::RunTools,
        adapter: "codex".into(),
        policy: yunta_core::port::PolicyApplied::Recorded(String::new()),
    };
    let payload = EventPayload::Session(SessionEvent::CapabilityDegraded(degraded));
    assert_eq!(said_for(payload), "run — run_tools not declared by codex");
}

#[test]
fn every_kind_earns_its_words_once() {
    // One sentence per kind, and every one of them a sentence: a
    // kind nothing says words for would reach a reader as a bare
    // subject, and a `Debug` spelling would reach them as a Rust
    // identifier.
    for payload in yunta_testkit_core::all_kinds() {
        let kind = payload.kind_name();
        let said = said_for(payload);
        assert!(
            said.contains('—') || said != "run",
            "`{kind}` says nothing beyond its subject: {said:?}"
        );
        assert!(
            !said.contains('{') && !said.contains("::"),
            "`{kind}` reached a reader as a Rust value: {said:?}"
        );
    }
}

/// Whether the terminal keeps `payload` above its region, as the only event
/// of a log.
fn kept_from_event(payload: EventPayload) -> bool {
    let events = vec![StoredEvent {
        seq: 1.into(),
        run_id: "01JQ0000000000000000000000".into(),
        node_id: Some("work".into()),
        timestamp: chrono::DateTime::UNIX_EPOCH,
        body: EventBody::Known(payload),
    }];
    kept(&derive_chronicle(&events)[0].happening)
}

#[test]
fn what_a_watched_terminal_keeps_is_what_closed_or_asked() {
    // Stated once: a terminal keeps above its region what
    // closed something or asked something of a person. Everything
    // else is the run working, and the region shows that while it
    // is true.
    let opened = EventPayload::Session(yunta_core::events::SessionEvent::Opened(
        serde_json::from_value(serde_json::json!({
            "kind": "agent_session_opened",
            "session_id": "s1",
            "capabilities": {},
        }))
        .expect("a session that opened"),
    ));
    assert!(
        !kept_from_event(opened),
        "a session opening is the run working"
    );

    let failed = EventPayload::Node(NodeEvent::Failed(
        yunta_core::events::NodeFailedPayload::new(
            Failure::message("exit 1"),
            TokenUsage::default(),
        ),
    ));
    assert!(
        kept_from_event(failed),
        "a node that failed closed something"
    );

    let failed_call = EventPayload::Session(yunta_core::events::SessionEvent::RunToolFailed(
        yunta_core::events::RunToolFailedPayload {
            session_id: "s1".into(),
            tool: yunta_core::RunTool::Submit(yunta_core::ArtifactKind::Questions),
            cause: yunta_core::events::RunToolFailureCause::CallFailed,
        },
    ));
    assert!(
        kept_from_event(failed_call),
        "a watched terminal keeps a failed call visible"
    );
}

/// A formatter the engine stopped at its bound is a stopped command, not a
/// failed one: its recorded code is the one a stopped command gets, so the
/// line says it was stopped and names no exit code.
#[test]
fn a_formatter_stopped_at_its_bound_reads_as_stopped_not_as_a_failed_exit() {
    let stopped = yunta_core::events::WriteFormattedPayload {
        session_id: "s1".into(),
        run: "fmt".into(),
        paths: vec!["a.rs".into()],
        changed: Vec::new(),
        exit_code: -2,
        duration_ms: 5_000,
        output: None,
        timed_out: true,
    };
    assert_eq!(
        said_for(EventPayload::Session(SessionEvent::WriteFormatted(stopped))),
        "run — `fmt` was stopped while formatting what it wrote"
    );
}
