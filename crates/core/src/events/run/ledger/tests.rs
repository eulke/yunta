use super::*;
use crate::events::{RunCreatedPayload, RunPausedPayload, RunResumedPayload};

fn environment(path: &str) -> ExecutionEnvironment {
    ExecutionEnvironment {
        shell: "/bin/sh".to_string(),
        path: vec![path.to_string()],
    }
}

fn at(ledger: &mut RunLedger, seq: u64, event: RunEvent) {
    let meta = EventMeta {
        seq: Seq::from(seq),
        at: chrono::DateTime::UNIX_EPOCH,
        node: None,
    };
    ledger.apply(&event, &meta);
}

/// Each seq is answered with what the invocation that wrote it ran
/// with: the birth's before the first wake, then each wake's own —
/// and nothing for a wake that recorded none.
#[test]
fn the_environment_at_a_seq_is_the_one_its_invocation_ran_with() {
    let mut ledger = RunLedger::default();
    let created = RunCreatedPayload {
        checkout: None,
        manifest_hash: crate::sha256_hex(b"manifest"),
        inputs: Default::default(),
        mode: Default::default(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: crate::sha256_hex(b"base").as_str().into(),
        environment: Some(Box::new(environment("/born"))),
        left_out: Vec::new(),
        opens_on_base: false,
    };
    at(&mut ledger, 1, RunEvent::Created(created));
    let woken = RunResumedPayload::new(Vec::new(), Some(environment("/woken")));
    at(&mut ledger, 5, RunEvent::Resumed(woken));
    at(
        &mut ledger,
        9,
        RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None)),
    );

    let path_at = |seq: u64| {
        ledger
            .environment_at(Seq::from(seq))
            .map(|in_force| in_force.path.join(":"))
    };
    assert_eq!(path_at(4).as_deref(), Some("/born"));
    assert_eq!(path_at(5).as_deref(), Some("/woken"));
    assert_eq!(path_at(8).as_deref(), Some("/woken"));
    assert_eq!(path_at(9), None);
    assert_eq!(ledger.environment_now(), None);
}

fn born_in(checkout: Option<&str>) -> RunCreatedPayload {
    RunCreatedPayload {
        checkout: checkout.map(std::path::PathBuf::from),
        manifest_hash: crate::sha256_hex(b"manifest"),
        inputs: Default::default(),
        mode: Default::default(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: crate::sha256_hex(b"base").as_str().into(),
        environment: None,
        left_out: Vec::new(),
        opens_on_base: false,
    }
}

/// The run's checkout is the one its log last named: where it was born,
/// unchanged by a wake that names none, and moved by one that does.
#[test]
fn the_checkout_is_the_one_the_log_last_named() {
    let mut ledger = RunLedger::default();
    at(
        &mut ledger,
        1,
        RunEvent::Created(born_in(Some("/pool/slot-1"))),
    );
    let still = RunResumedPayload::new(Vec::new(), None);
    at(&mut ledger, 2, RunEvent::Resumed(still));
    assert_eq!(
        ledger.checkout(),
        Some(std::path::Path::new("/pool/slot-1"))
    );

    let moved = RunResumedPayload::new(Vec::new(), None).in_checkout("/pool/slot-3".into());
    at(&mut ledger, 3, RunEvent::Resumed(moved));

    assert_eq!(
        ledger.checkout(),
        Some(std::path::Path::new("/pool/slot-3"))
    );
}

/// Only the wake that moved the run says so: the next one, in the same
/// checkout, does not.
#[test]
fn a_wake_that_moved_the_run_says_so_and_the_next_does_not() {
    let mut ledger = RunLedger::default();
    at(
        &mut ledger,
        1,
        RunEvent::Created(born_in(Some("/pool/slot-1"))),
    );
    let moved = RunResumedPayload::new(Vec::new(), None).in_checkout("/pool/slot-2".into());
    at(&mut ledger, 2, RunEvent::Resumed(moved));
    assert!(ledger.moved_on_waking());

    at(
        &mut ledger,
        3,
        RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None)),
    );

    assert!(!ledger.moved_on_waking());
}

/// A log written before the field names no checkout, and reads back.
#[test]
fn an_old_log_names_no_checkout() {
    let written = serde_json::to_value(born_in(None)).unwrap();
    assert!(written.get("checkout").is_none(), "{written}");
    let read: RunCreatedPayload = serde_json::from_value(written).unwrap();
    let mut ledger = RunLedger::default();
    at(&mut ledger, 1, RunEvent::Created(read));

    assert_eq!(ledger.checkout(), None);
}

/// A checkout the log names round-trips as written.
#[test]
fn run_created_round_trips_its_checkout() {
    let born = born_in(Some("/pool/slot-1"));
    let written = serde_json::to_string(&born).unwrap();
    let read: RunCreatedPayload = serde_json::from_str(&written).unwrap();
    assert_eq!(read, born);
}

/// A pause reads back as the fact its event recorded, until a resume
/// after it ends the pause.
#[test]
fn a_pause_is_the_fact_it_recorded_until_a_resume_follows() {
    let mut ledger = RunLedger::default();
    at(&mut ledger, 1, RunEvent::Created(born_in(None)));
    let pause = PauseReason::BudgetExhausted { spent: 5, cap: 4 };
    at(
        &mut ledger,
        2,
        RunEvent::Paused(RunPausedPayload::new(&pause)),
    );
    assert_eq!(ledger.paused(), Some((&pause, Seq::from(2_u64))));

    let resumed = RunResumedPayload::new(Vec::new(), None);
    at(&mut ledger, 3, RunEvent::Resumed(resumed));
    assert_eq!(ledger.paused(), None);
}

/// A wake logged at `seq`, `seconds` after the epoch, that found the run at
/// `tree` when it found one.
fn woke(ledger: &mut RunLedger, seq: u64, seconds: i64, tree: Option<&str>) {
    let mut resumed = RunResumedPayload::new(Vec::new(), None);
    if let Some(tree) = tree {
        resumed = resumed.finding(tree.parse().unwrap());
    }
    let meta = EventMeta {
        seq: Seq::from(seq),
        at: DateTime::UNIX_EPOCH + chrono::Duration::seconds(seconds),
        node: None,
    };
    ledger.apply(&RunEvent::Resumed(resumed), &meta);
}

/// The latest wake names the tree it found, with its seq and the time it was
/// logged; a birth names neither.
#[test]
fn the_latest_wake_names_the_tree_it_found_with_its_seq_and_time() {
    let mut ledger = RunLedger::default();
    at(&mut ledger, 1, RunEvent::Created(born_in(None)));
    assert_eq!(ledger.woke_on(), None);
    assert_eq!(ledger.resumed_at(), None);

    woke(&mut ledger, 5, 60, Some("a1a1a1a"));
    let tree: TreeId = "a1a1a1a".parse().unwrap();
    assert_eq!(ledger.woke_on(), Some((Seq::from(5_u64), &tree)));
    assert_eq!(
        ledger.resumed_at(),
        Some(DateTime::UNIX_EPOCH + chrono::Duration::seconds(60))
    );
}

/// A later wake that found no tree names none, and still says when it woke.
#[test]
fn a_later_wake_that_found_no_tree_names_none_and_still_says_when_it_woke() {
    let mut ledger = RunLedger::default();
    woke(&mut ledger, 5, 60, Some("a1a1a1a"));
    woke(&mut ledger, 9, 90, None);

    assert_eq!(ledger.woke_on(), None);
    assert_eq!(
        ledger.resumed_at(),
        Some(DateTime::UNIX_EPOCH + chrono::Duration::seconds(90))
    );
}
