use std::collections::BTreeMap;

use surco::look::Look;
use surco::paint::Paint;
use yunta_core::events::{
    EventBody, EventPayload, Failure, NodeEvent, RailFailedPayload, RailSummaryPayload, RunEvent,
    SessionEvent, StoredEvent, TokenUsage,
};
use yunta_core::rail::hook::RailFailureCause;
use yunta_core::rail::timing::Histogram;
use yunta_core::rail::RailEvent;
use yunta_render::chronicle;
use yunta_replay::frame::chronicle as derive_chronicle;
use yunta_testkit_core::Log;

use super::{graduation, kept};

/// One run's moments two ways, as the two layouts lay them out: every line the
/// append-only surface writes, and the rows a watched terminal keeps above its
/// region, each read plain and without its mark.
fn written_and_kept() -> (Vec<String>, Vec<String>) {
    let moments = derive_chronicle(&lint_failed());
    let written = moments
        .iter()
        .map(|moment| chronicle::say(moment, surco::glyphs::Glyphs::Ascii).text)
        .collect();
    let history = moments
        .iter()
        .filter(|moment| kept(&moment.happening))
        .flat_map(|moment| {
            graduation(
                moment,
                &yunta_core::RunId::from_static("01JQ0000000000000000000000"),
                &Look::plain(),
            )
        })
        .map(|row| {
            Paint::Plain
                .line(&row)
                .trim_start_matches(['x', '+', '>', '!', '?', ' '])
                .to_string()
        })
        .collect();
    (written, history)
}

/// A node that started, failed, and had its context assembled after: a moment that
/// closed something between two that are the run working.
fn lint_failed() -> Vec<StoredEvent> {
    Log::for_run("01JQ0000000000000000000000")
        .node(
            "lint",
            EventPayload::Node(NodeEvent::Started(
                yunta_core::events::NodeStartedPayload::attempt(1),
            )),
        )
        .after(1)
        .node(
            "lint",
            EventPayload::Node(NodeEvent::Failed(
                yunta_core::events::NodeFailedPayload::new(
                    Failure::Message {
                        outcome: "exit 1".into(),
                    },
                    TokenUsage::default(),
                ),
            )),
        )
        .after(1)
        .node(
            "lint",
            EventPayload::Node(NodeEvent::ContextAssembled(
                serde_json::from_value(serde_json::json!({
                    "sources": [],
                    "segment_hashes": {},
                }))
                .expect("a context that assembled from nothing"),
            )),
        )
        .build()
}

/// One derivation, two layouts. A reader who followed a run on a terminal and a
/// reader who read the same run out of a pipe met the same sentences: what the
/// terminal kept is a subsequence of what the pipe wrote, in the same order and
/// word for word.
#[test]
fn what_a_watched_terminal_keeps_above_its_region_is_what_the_append_only_surface_writes() {
    let (written, history) = written_and_kept();
    let mut rest = written.iter();
    for kept in &history {
        assert!(
            rest.any(|line| line == kept),
            "the terminal kept a sentence the pipe never wrote, or wrote it out of order: \
             {kept:?} in {written:?}"
        );
    }
}

/// The region shows the run working while it is true, so the history above it
/// keeps less than a pipe, which has no region, writes.
#[test]
fn a_watched_terminal_keeps_less_than_the_append_only_surface_writes() {
    let (written, history) = written_and_kept();
    assert!(
        history.len() < written.len(),
        "a terminal keeps less than a pipe writes: {history:?} vs {written:?}"
    );
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
            Failure::Message {
                outcome: "exit 1".into(),
            },
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

/// A machine that slept stays above the region that keeps moving.
#[test]
fn a_host_suspension_stays_above_the_live_region() {
    let payload = EventPayload::Run(RunEvent::HostSuspended(
        yunta_core::events::HostSuspendedPayload::slept(std::time::Duration::from_secs(38 * 60)),
    ));
    assert!(kept_from_event(payload));
}

/// A failed call of the rail is a moment a person can act on: a watched
/// terminal keeps it above its region.
#[test]
fn a_rail_failure_is_kept() {
    let failed = EventPayload::Session(SessionEvent::RailFailed(RailFailedPayload {
        session_id: Some("s1".into()),
        event: Some(RailEvent::BeforeWrite),
        cause: RailFailureCause::Timeout,
    }));
    assert!(
        kept_from_event(failed),
        "a failed rail call closed something"
    );
}

/// What the rail heard of a session is a record, not a failure: a watched
/// terminal keeps none of it above its region.
#[test]
fn a_rail_summary_is_not_kept() {
    let summary = EventPayload::Session(SessionEvent::RailSummary(RailSummaryPayload {
        session_id: "s1".into(),
        model: Some("claude-haiku-4-5".into()),
        moments: BTreeMap::from([(RailEvent::BeforeWrite, Histogram::default())]),
    }));
    assert!(
        !kept_from_event(summary),
        "a summary of the rail is the run working"
    );
}

/// A formatter the bound stopped is the project's to fix, so a watched
/// terminal keeps it; one that finished is the run working.
#[test]
fn a_stopped_formatter_is_kept_and_a_finished_one_is_not() {
    let formatted = |timed_out: bool| {
        EventPayload::Session(SessionEvent::WriteFormatted(
            yunta_core::events::WriteFormattedPayload {
                session_id: "s1".into(),
                run: "fmt".into(),
                paths: vec!["a.rs".into()],
                changed: Vec::new(),
                exit_code: if timed_out { -2 } else { 0 },
                duration_ms: 5_000,
                output: None,
                timed_out,
            },
        ))
    };
    assert!(
        kept_from_event(formatted(true)),
        "a stopped formatter is kept"
    );
    assert!(
        !kept_from_event(formatted(false)),
        "a finished formatter is not kept"
    );
}
