//! Logs a property draws: well-formed events — a valid payload per kind,
//! every kind the wire knows plus one it does not, `seq` in order — with no
//! lifecycle enforced, so a property that holds over them holds over any log
//! a reader can be handed, not just a tidy one.
//!
//! One generator for every crate that folds a log: core's ledgers, replay
//! and the engine's store each state their properties over the same logs.

use proptest::prelude::*;
use yunta_core::events::{
    ArtifactId, EventBody, EventPayload, FindingSeverity, StoredEvent, TaskStatus, TokenUsage,
    UnknownEvent,
};
use yunta_core::ArtifactKind;

use crate::{all_kinds, Log};

mod kinds;
use kinds::parameterized;

/// The run every generated log is of.
pub const RUN: &str = "run-prop";

/// Usage whose cache is a part of its input: the cached tokens never exceed
/// the input tokens they are counted in.
pub fn tokens() -> impl Strategy<Value = TokenUsage> {
    (0u64..1000, 0u64..1000).prop_flat_map(|(input, output)| {
        proptest::option::of(0..=input).prop_map(move |cached| TokenUsage {
            input,
            output,
            cached,
        })
    })
}

fn task_status() -> impl Strategy<Value = TaskStatus> {
    prop_oneof![
        Just(TaskStatus::Pending),
        Just(TaskStatus::Ready),
        Just(TaskStatus::Running),
        Just(TaskStatus::Done),
        Just(TaskStatus::Blocked),
        Just(TaskStatus::Failed),
    ]
}

fn severity() -> impl Strategy<Value = FindingSeverity> {
    prop_oneof![
        Just(FindingSeverity::Blocking),
        Just(FindingSeverity::Major),
        Just(FindingSeverity::Minor),
        Just(FindingSeverity::Note),
    ]
}

/// A task id drawn from a tiny pool, so generated logs actually revisit the
/// same task and `RunState::replay` folds real transitions rather than a sea of
/// singletons.
fn task_id() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("t1"), Just("t2"), Just("t3")]
}

/// The payload one generated event carries: every kind an event can be
/// written under, drawn from the one list that names them all, and the
/// handful of kinds [`parameterized`] varies on purpose. Half the draws
/// come from each, so no kind is unreachable and the kinds a property
/// needs to see repeat still repeat.
fn payload() -> impl Strategy<Value = EventPayload> {
    prop_oneof![prop::sample::select(all_kinds()), parameterized(),]
}

/// An artifact identity drawn from a tiny pool, so generated logs
/// actually accept the same identity twice and the fold folds real
/// replacements rather than a sea of singletons.
fn artifact_id() -> impl Strategy<Value = ArtifactId> {
    prop_oneof![
        Just(ArtifactId::Interpreted {
            kind: ArtifactKind::Tasks
        }),
        Just(ArtifactId::Interpreted {
            kind: ArtifactKind::Findings
        }),
        Just(ArtifactId::Opaque {
            name: "notes.md".to_string()
        }),
    ]
}

/// One log entry: an optional node id (from a small pool, so nodes
/// recur), the payload it carries, and the kind it was written under
/// when that kind is one this binary does not know. `seq` is assigned in
/// order when the log is built.
pub fn entry() -> impl Strategy<Value = (Option<&'static str>, EventPayload, Option<&'static str>)>
{
    let node = prop_oneof![
        Just(None),
        Just(Some("a")),
        Just(Some("b")),
        Just(Some("c"))
    ];
    (node, payload(), unknown_kind())
}

/// The kind a generated event is written under when no binary of this
/// version knows it — what a log a newer writer left behind holds, and
/// what a reader counts and skips. `None` for an event this binary
/// interprets, which is most of them.
fn unknown_kind() -> impl Strategy<Value = Option<&'static str>> {
    prop_oneof![
        8 => Just(None),
        1 => Just(Some("node_teleported")),
        1 => Just(Some("run_reticulated")),
    ]
}

/// A whole generated log, stated in the order it was drawn. The clock
/// stands still across it: a log's derived state must not depend on wall
/// time.
///
/// An entry that names an unknown kind keeps the envelope the builder
/// gave it — run, position, instant, node — and carries that kind in
/// place of a payload, which is exactly the shape the store reads back
/// for an event this binary cannot interpret.
pub fn log() -> impl Strategy<Value = Vec<StoredEvent>> {
    prop::collection::vec(entry(), 0..40).prop_map(|entries| {
        let kinds: Vec<Option<&'static str>> = entries.iter().map(|(_, _, kind)| *kind).collect();
        let mut events = entries
            .into_iter()
            .fold(Log::for_run(RUN), |log, (node, payload, _)| match node {
                Some(node) => log.node(node, payload),
                None => log.event(payload),
            })
            .build();
        for (event, kind) in events.iter_mut().zip(kinds) {
            if let Some(kind) = kind {
                event.body = EventBody::Unknown(UnknownEvent {
                    kind: kind.to_string(),
                    schema_version: 1,
                    payload: serde_json::Map::new(),
                });
            }
        }
        events
    })
}

/// The artifacts a run accepted, each with the bytes behind it — so a
/// test can fill an object store with exactly what the log names.
pub fn accepted_artifacts() -> impl Strategy<Value = Vec<(Option<&'static str>, ArtifactId, String)>>
{
    let node = prop_oneof![Just(None), Just(Some("a")), Just(Some("b"))];
    prop::collection::vec((node, artifact_id(), "[a-z]{1,8}"), 0..12)
}
