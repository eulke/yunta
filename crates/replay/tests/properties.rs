//! Property tests for the replay guarantees the whole engine rests on:
//! `RunState::replay` is a pure, deterministic, prefix-monotonic fold of the
//! event log, at every length and not just at the full log; a log folded in
//! any chunks derives what its replay derives, past a break too, and an
//! event already taken moves nothing; a second copy of an event that states
//! what the run holds derives what one copy derives; and the chronicle reads
//! one log as replay reads it (its own module).
//!
//! The logs come from `yunta_testkit_core::arbitrary`, which enforces no
//! valid *lifecycle*: `RunState::replay` interprets any log (marking one it
//! cannot follow `broken`), so these properties hold over arbitrary logs.

use std::time::Duration;

use proptest::prelude::*;
use yunta_core::events::node::happening::Happening as NodeHappening;
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, EngineFact, EventPayload, FindingEvent,
    HostSuspendedPayload, NodeFinishedPayload, NodeOutcome, NodeStartedPayload, RecordedOrigin,
    StoredEvent, TokenUsage,
};
use yunta_core::events::{ArtifactEvent, GateEvent, NodeEvent, RunEvent};
use yunta_core::ArtifactKind;
use yunta_replay::frame::{chronicle, Happening, Moment};
use yunta_replay::RunState;
use yunta_testkit_core::arbitrary::{entry, log, tokens, RUN};
use yunta_testkit_core::Log;

#[path = "properties/chronicle.rs"]
mod chronicle;

/// What the node ledger counts — an attempt open, sessions without their
/// service — is what its records hold.
fn counts_what_its_records_hold(state: &RunState) -> Result<(), TestCaseError> {
    let open = || state.nodes.records().filter(|r| r.open_since.is_some());
    prop_assert_eq!(state.nodes.attempting(), open().next().is_some());
    let offline = state.nodes.records().any(|r| r.unreachable);
    let waiting = offline && open().all(|r| r.unreachable);
    prop_assert_eq!(state.nodes.attempts_offline(), waiting);
    Ok(())
}

/// One node's whole ask round: it starts, hands its questions over,
/// waits, is answered, and takes the terminal its close deferred.
fn a_questions_round() -> Vec<StoredEvent> {
    let questions = yunta_core::sha256_hex(b"questions");
    let answers = yunta_core::sha256_hex(b"answers");
    Log::for_run(RUN)
        .node(
            "grill",
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        )
        .node(
            "grill",
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                ArtifactId::Interpreted {
                    kind: ArtifactKind::Questions,
                },
                questions.clone(),
                RecordedOrigin::Submitted,
            ))),
        )
        .node(
            "grill",
            EventPayload::Gates(GateEvent::QuestionsAsked(
                yunta_core::events::QuestionsAskedPayload::new(
                    questions,
                    vec!["q1".into()],
                    yunta_core::events::TokenUsage {
                        input: 30,
                        output: 12,
                        cached: None,
                    },
                )
                .expect("one question is a question"),
            )),
        )
        .node(
            "grill",
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                ArtifactId::Opaque {
                    name: "answers.yaml".to_string(),
                },
                answers.clone(),
                RecordedOrigin::Answered,
            ))),
        )
        .node(
            "grill",
            EventPayload::Gates(GateEvent::QuestionsAnswered(
                yunta_core::events::QuestionsAnsweredPayload {
                    answers_hash: answers,
                    channel: yunta_core::events::Channel::Tty,
                    responder: None,
                },
            )),
        )
        .node(
            "grill",
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                NodeOutcome::Fact(EngineFact::QuestionsAnswered),
                yunta_core::events::TokenUsage::default(),
            ))),
        )
        .build()
}

/// A log and the points it is cut at, each a position in it, as a reader
/// that follows a run takes it: one read at a time, from where the last
/// one stopped.
fn cut(log: Vec<StoredEvent>) -> impl Strategy<Value = (Vec<StoredEvent>, Vec<usize>)> {
    let len = log.len();
    (Just(log), prop::collection::vec(0..=len, 0..6))
}

/// A log whose middle holds an event no log can take: a node's terminal
/// that names no node. What comes before and after it is drawn, and every
/// event keeps its own position, counted from 1.
fn a_broken_log() -> impl Strategy<Value = Vec<StoredEvent>> {
    (log(), log()).prop_map(|(before, after)| {
        let refused = Log::for_run(RUN)
            .event(EventPayload::Node(NodeEvent::Finished(
                NodeFinishedPayload::new(NodeOutcome::Summary("ok".into()), TokenUsage::default()),
            )))
            .build();
        let mut events: Vec<StoredEvent> = before.into_iter().chain(refused).chain(after).collect();
        for (position, event) in events.iter_mut().enumerate() {
            event.seq = (position as u64 + 1).into();
        }
        events
    })
}

/// `log` cut at each of `cuts`, in order: the reads a reader that follows
/// the run takes it in.
fn chunks<'a>(log: &'a [StoredEvent], cuts: &[usize]) -> Vec<&'a [StoredEvent]> {
    let mut ends = cuts.to_vec();
    ends.sort_unstable();
    ends.push(log.len());
    let mut from = 0;
    ends.into_iter()
        .map(|to| {
            let chunk = &log[from..to];
            from = to;
            chunk
        })
        .collect()
}

/// `log` folded into a fresh state chunk by chunk, cut at each of `cuts`.
fn folded_in_chunks(log: &[StoredEvent], cuts: &[usize]) -> RunState {
    let mut state = RunState::default();
    for chunk in chunks(log, cuts) {
        for event in chunk {
            state.fold(event);
        }
    }
    state
}

/// What every cut of a round derives, and therefore what a resume does
/// from there: the node's state at each length of the log.
///
/// The table is the point. A crash after the node asked leaves it
/// waiting, so a resume asks again from the document it already holds; a
/// crash after the answer leaves it running and owed a terminal, so a
/// resume pays that terminal from the log. At no cut does the node come
/// back as something to re-run, which is what makes an interrupted round
/// cost an append rather than a session.
#[test]
fn every_cut_of_a_questions_round_derives_what_a_resume_acts_on() {
    let round = a_questions_round();
    let grill = yunta_core::NodeId::from("grill");
    let asked_tokens = yunta_core::events::TokenUsage {
        input: 30,
        output: 12,
        cached: None,
    };

    let at = |k: usize| RunState::replay(&round[..k]);
    assert_eq!(at(0).nodes.state(&grill), None, "before it started");
    for k in 1..=2 {
        assert!(
            matches!(
                at(k).nodes.state(&grill),
                Some(yunta_core::events::NodeState::Running { attempt: 1 })
            ),
            "running at {k}: {:?}",
            at(k).nodes.state(&grill)
        );
        assert!(!at(k).answered_unfinished(&grill));
    }
    for k in 3..=4 {
        assert!(
            matches!(
                at(k).nodes.state(&grill),
                Some(yunta_core::events::NodeState::Waiting {
                    on: yunta_core::events::NodeWait::Questions { .. }
                })
            ),
            "waiting on its questions at {k}: {:?}",
            at(k).nodes.state(&grill)
        );
        assert_eq!(
            at(k).spend().so_far(),
            asked_tokens,
            "the session that asked is counted once, at {k}"
        );
        assert_eq!(
            at(k).spend().settled,
            asked_tokens,
            "the round settled the usage of the session that asked, at {k}"
        );
    }
    assert!(
        matches!(
            at(5).nodes.state(&grill),
            Some(yunta_core::events::NodeState::Running { attempt: 1 })
        ),
        "answered, back where asking left it: {:?}",
        at(5).nodes.state(&grill)
    );
    assert!(
        at(5).answered_unfinished(&grill),
        "and owed the terminal its close deferred"
    );
    assert_eq!(
        at(6).nodes.state(&grill),
        Some(&yunta_core::events::NodeState::Finished {
            outcome: NodeOutcome::Fact(EngineFact::QuestionsAnswered),
            tokens: asked_tokens,
        }),
        "finished, carrying what the session that asked spent"
    );
    assert!(!at(6).answered_unfinished(&grill));
    assert_eq!(
        at(6).spend().so_far(),
        asked_tokens,
        "counted once, not twice"
    );
}

proptest! {
    /// `RunState::replay` is pure: the same log always derives the same state, with no
    /// dependence on `HashMap` iteration order or any hidden state.
    /// Reached by: `crates/replay/src/lib.rs`, `crates/replay/src/fold.rs`,
    /// `crates/replay/src/activity.rs`,
    /// `crates/replay/src/clock.rs`, `crates/replay/src/decisions.rs`,
    /// `crates/replay/src/refuse.rs`, `crates/replay/src/tree.rs`,
    /// `crates/core/src/events/**`
    #[test]
    fn replay_is_deterministic(log in log()) {
        prop_assert_eq!(RunState::replay(&log), RunState::replay(&log));
    }

    /// `RunState::replay` is monotonic by prefix: as the log grows, settled tokens
    /// only accumulate, tasks and nodes once seen stay seen, and a finding
    /// leaves the set that stands only by being withdrawn — a longer
    /// prefix never un-does what a shorter one derived.
    #[test]
    fn replay_is_prefix_monotonic(log in log()) {
        // Every node's settled usage and every closed child run's total:
        // what the log has settled, with the usage in flight left out.
        let settled = |state: &RunState| -> u64 {
            let nodes: u64 = state
                .nodes
                .values()
                .map(|record| record.spend.settled.total())
                .sum();
            nodes + state.children.tokens().total()
        };
        let mut prev = RunState::replay(&[]);
        for k in 1..=log.len() {
            let cur = RunState::replay(&log[..k]);
            // Usage in flight is not monotone: a restart drops what an open
            // attempt reported, and a close replaces it with what it states.
            prop_assert!(settled(&cur) >= settled(&prev));
            // The one event this prefix added is the only thing that can
            // have taken a finding back, so the property reads it rather
            // than the ledger: a run-level finding has no node to ask
            // about, and it is withdrawn the same way any other is.
            let added = &log[k - 1];
            for posted in prev.findings.effective() {
                let stands = cur
                    .findings
                    .effective()
                    .iter()
                    .any(|now| now.node == posted.node && now.finding.id == posted.finding.id);
                let withdrawn = added.node_id == posted.node
                    && matches!(
                        added.payload(),
                        Some(EventPayload::Findings(FindingEvent::Withdrawn(taken)))
                            if taken.id == posted.finding.id
                    );
                prop_assert!(
                    stands || withdrawn,
                    "finding {} left the set without a withdrawal",
                    posted.finding.id
                );
            }
            for id in prev.tasks.iter().map(|(id, _)| id) {
                prop_assert!(cur.tasks.contains(id), "task {id} disappeared");
            }
            for id in prev.nodes.iter().map(|(id, _)| id) {
                prop_assert!(cur.nodes.contains(id), "node {id} disappeared");
            }
            counts_what_its_records_hold(&cur)?;
            prev = cur;
        }
    }

    /// `RunState::replay` is deterministic at every length, not just at the full
    /// log: the same prefix derived twice is the same state. Nothing
    /// outside the events — an iteration order, a clock, a counter kept
    /// between calls — reaches the result of a partial log either.
    #[test]
    fn replay_is_deterministic_from_any_prefix(
        (log, k) in log().prop_flat_map(|log| {
            let n = log.len();
            (Just(log), 0..=n)
        })
    ) {
        let prefix = &log[..k];
        prop_assert_eq!(RunState::replay(prefix), RunState::replay(prefix));
    }

    /// A reader that folds a run's log as it comes, a few events at a time,
    /// holds the state a reader of the whole log derives, and stands at the
    /// same head: where the reads were cut changes nothing.
    #[test]
    fn folding_a_log_in_any_chunks_equals_replaying_it(
        (log, cuts) in log().prop_flat_map(cut)
    ) {
        let folded = folded_in_chunks(&log, &cuts);
        let replayed = RunState::replay(&log);
        prop_assert_eq!(&folded, &replayed);
        prop_assert_eq!(folded.head(), replayed.head());
        prop_assert_eq!(folded.head(), log.last().map(|event| event.seq));
    }

    /// A log that breaks in its middle folds, chunk by chunk, to the break
    /// its replay names and to nothing past it, while its head goes on to
    /// its last event: a reader never reads a broken log's tail again.
    #[test]
    fn a_broken_log_folds_as_it_replays(
        (log, cuts) in a_broken_log().prop_flat_map(cut)
    ) {
        let folded = folded_in_chunks(&log, &cuts);
        let replayed = RunState::replay(&log);
        prop_assert!(replayed.broken.is_some(), "the event that names no node breaks the log");
        prop_assert_eq!(&folded.broken, &replayed.broken);
        prop_assert_eq!(&folded, &replayed);
        prop_assert_eq!(folded.head(), log.last().map(|event| event.seq));
    }

    /// An event at or below the head was taken already: a log folded, then
    /// folded again whole, derives what it derived once.
    #[test]
    fn an_event_already_taken_moves_nothing(log in log()) {
        let once = RunState::replay(&log);
        let mut again = once.clone();
        for event in &log {
            again.fold(event);
        }
        prop_assert_eq!(&again, &once);
        prop_assert_eq!(again.head(), once.head());
    }

    /// A second copy of an event that states what the run holds derives
    /// what one copy derives: a channel that hands the same acceptance or
    /// the same posting over twice leaves the run holding exactly what it
    /// held.
    ///
    /// Each copy is the event it repeats, position included — the two
    /// arrivals are one event, which is what tells a redelivery apart
    /// from a second acceptance of the same identity. So the copy stands
    /// at the head the first arrival moved the fold to, and the fold takes
    /// it as taken already before any ledger reads it.
    #[test]
    fn redelivering_any_subsequence_of_events_changes_no_state(
        (log, again) in log().prop_flat_map(|log| {
            let len = log.len();
            (Just(log), prop::collection::vec(any::<bool>(), len))
        })
    ) {
        let mut redelivered = Vec::new();
        for (event, again) in log.iter().zip(again) {
            redelivered.push(event.clone());
            let holdable = matches!(
                event.payload(),
                Some(EventPayload::Artifacts(_) | EventPayload::Findings(_))
            );
            if again && holdable {
                redelivered.push(event.clone());
            }
        }
        prop_assert_eq!(RunState::replay(&redelivered), RunState::replay(&log));
    }

    /// The cache is a part of the input: whatever a node's terminal reports,
    /// the spend a log derives counts no more cached tokens than input tokens.
    #[test]
    fn the_cache_is_a_part_of_the_input(usage in tokens()) {
        let events = Log::for_run(RUN)
            .node(
                "a",
                EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            )
            .node(
                "a",
                EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                    NodeOutcome::Summary("ok".into()),
                    usage,
                ))),
            )
            .build();
        let spent = RunState::replay(&events).spend().so_far();
        prop_assert!(spent.cached.unwrap_or(0) <= spent.input);
    }

    /// Every stretch of a clocked log is booked once: the buckets add up to
    /// the awake time from the log's first event to its last.
    #[test]
    fn every_stretch_of_a_clocked_log_is_booked_once(
        steps in prop::collection::vec((0i64..120, prop::option::of(0u64..300), entry()), 1..30)
    ) {
        let mut log = Log::for_run(RUN);
        for (gap, slept, (node, payload, _)) in steps {
            // A suspension comes only from `slept`, inside its gap: a host
            // suspension drawn among the kinds would state a sleep no gap bounds.
            if matches!(payload, EventPayload::Run(RunEvent::HostSuspended(_))) {
                continue;
            }
            log = log.after(gap);
            if let Some(secs) = slept {
                // A host sleeps inside the gap it is stated in, so no two
                // suspensions overlap.
                let slept = Duration::from_secs(secs.min(gap as u64));
                log = log.event(EventPayload::Run(RunEvent::HostSuspended(
                    HostSuspendedPayload::slept(slept),
                )));
            }
            log = match node {
                Some(n) => log.node(n, payload),
                None => log.event(payload),
            };
        }
        let state = RunState::replay(&log.build());
        prop_assert_eq!(
            state.time_spent(None).total(),
            state.elapsed(None).unwrap_or_default()
        );
    }
}
