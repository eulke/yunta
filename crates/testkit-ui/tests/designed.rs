//! The design's run, recorded again in a place of its own, writes the log and the run
//! directory kept for it, so a change to what the engine writes for any moment of that
//! run fails here; and the kept run passes through the moments the design draws, each
//! node working as long as the design shows it. Run with `YUNTA_BLESS=1`, the recording
//! is kept in place of the kept run, whose `run/` is removed first so no file outlives
//! the recording that made it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use surco::golden::assert_golden;
use yunta_core::events::{CriterionResult, CriterionType, Phase, StoredEvent};
use yunta_replay::RunState;
use yunta_testkit_ui::designed;

/// The kept log's events, one JSON object each, in the order the log holds them.
fn kept() -> Vec<Value> {
    designed::log()
        .lines()
        .map(|line| serde_json::from_str(line).expect("an event of the kept log"))
        .collect()
}

/// Whether `event` is of `kind` and names `node`.
fn is(event: &Value, kind: &str, node: &str) -> bool {
    event["kind"] == kind && event["node_id"] == node
}

/// The instant `event` stands at.
fn at(event: &Value) -> DateTime<Utc> {
    event["timestamp"]
        .as_str()
        .and_then(|stamp| stamp.parse().ok())
        .expect("an event's instant")
}

/// The seconds between the first event of `kind` naming `node` from `start`, and the
/// first event of `end` naming it after that.
fn worked(events: &[Value], node: &str, start: &str, end: &str) -> i64 {
    let from = events
        .iter()
        .position(|event| is(event, start, node))
        .unwrap_or_else(|| panic!("`{node}` has no `{start}`"));
    let to = events[from..]
        .iter()
        .find(|event| is(event, end, node))
        .unwrap_or_else(|| panic!("`{node}` has no `{end}` after its `{start}`"));
    (at(to) - at(&events[from])).num_seconds()
}

/// Every file under `dir`, by its path under `root`.
fn files_under(root: &Path, dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
        return Vec::new();
    };
    entries
        .map(|entry| dir.join(entry.expect("a kept entry").file_name()))
        .flat_map(|path| match root.join(&path).is_dir() {
            true => files_under(root, &path),
            false => vec![path],
        })
        .collect()
}

#[test]
fn the_designs_run_recorded_again_writes_the_kept_run() {
    yunta_testkit::on_a_deep_stack(|| async {
        let recording = designed::record().await;
        let files = recording.files();
        for (path, text) in &files {
            assert_golden(&designed::kept().join(path), text);
        }
        let recorded: BTreeSet<&PathBuf> = files.iter().map(|(path, _)| path).collect();
        for kept in files_under(&designed::kept(), Path::new(designed::RUN_DIR)) {
            assert!(
                recorded.contains(&kept),
                "`{}` is kept, and the recording leaves no such file",
                kept.display()
            );
        }

        let receipt = recording.receipt().await;
        assert!(receipt.criteria.total > 0, "{:?}", receipt.criteria);
        assert_eq!(receipt.criteria.green, receipt.criteria.total);
        let baseline = receipt.baseline.as_ref().expect("a baseline");
        assert!(baseline.compared > 0, "{baseline:?}");
        assert!(receipt.scope.files_touched > 0, "{:?}", receipt.scope);
        assert_eq!(receipt.blocking_findings, 0);
    });
}

#[test]
fn three_reviewers_are_at_work_at_one_moment_of_the_kept_log() {
    let reviewers = BTreeSet::from(["conform", "review@reviewer", "review@reviewer-alt"]);
    let mut working = BTreeSet::new();
    let mut together = None;
    for event in kept() {
        let node = event["node_id"].as_str().unwrap_or_default();
        match event["kind"].as_str() {
            Some("node_started") => {
                working.insert(node.to_string());
            }
            Some("node_finished" | "node_failed") => {
                working.remove(node);
            }
            _ => {}
        }
        if reviewers.iter().all(|reviewer| working.contains(*reviewer)) {
            together.get_or_insert(event["seq"].clone());
        }
    }
    assert!(together.is_some(), "the three reviewers never work at once");
}

#[test]
fn the_kept_run_passes_through_the_moments_the_design_draws() {
    let events = kept();
    let kinds = |kind: &str| events.iter().filter(|event| event["kind"] == kind).count();

    let asked = events
        .iter()
        .find(|event| event["kind"] == "questions_asked")
        .expect("a round asked");
    assert_eq!(asked["questions"].as_array().map(Vec::len), Some(3));
    assert_eq!(kinds("questions_answered"), 1);
    for gate in ["approve-plan", "ship"] {
        assert!(
            events.iter().any(|event| is(event, "gate_resolved", gate)),
            "`{gate}` is answered"
        );
    }
    assert_eq!(kinds("scope_expansion_requested"), 2);
    assert_eq!(kinds("scope_expansion_granted"), 2);
    let mut severities: Vec<&str> = events
        .iter()
        .filter(|event| event["kind"] == "finding_posted")
        .filter_map(|event| event["finding"]["severity"].as_str())
        .collect();
    severities.sort_unstable();
    assert_eq!(severities, ["blocking", "major", "major", "minor"]);
    assert_eq!(kinds("finding_answered"), 4);
    assert!(events
        .iter()
        .any(|event| is(event, "node_rerouted", "lint")));
    let tests_failed = events
        .iter()
        .position(|event| is(event, "node_failed", "tests"))
        .expect("the suite fails");
    assert!(events[tests_failed..]
        .iter()
        .any(|event| is(event, "node_finished", "tests")));
    assert_eq!(kinds("pull_request_opened"), 1);
    assert_eq!(
        events.last().map(|event| event["kind"].clone()),
        Some(Value::from("run_finished"))
    );
}

#[test]
fn each_node_of_the_kept_run_works_as_long_as_the_design_shows() {
    let events = kept();
    for (node, end, seconds) in [
        ("grill", "questions_asked", 36),
        ("brief", "node_finished", 22),
        ("plan", "node_finished", 6 * 60 + 25),
        ("spec", "node_finished", 13 * 60 + 53),
        ("review@reviewer", "node_finished", 2 * 60),
        ("review@reviewer-alt", "node_finished", 42),
        ("conform", "node_finished", 25),
        ("fix-findings", "node_finished", 5 * 60 + 31),
        ("fix-lint", "node_finished", 60 + 31),
        ("tests", "node_failed", 32),
    ] {
        assert_eq!(
            worked(&events, node, "node_started", end),
            seconds,
            "`{node}` works as long as the design shows"
        );
    }
}

/// The design's run's one task.
const TASK: &str = "global-pack-install";

/// How many guards the design's task's checks run beside its own criterion: the suite and
/// the lint.
const GUARDS: usize = 2;

#[test]
fn the_kept_runs_task_holds_its_check_before_the_work_its_criterion_red_its_guards_green() {
    let events: Vec<StoredEvent> = designed::log()
        .lines()
        .map(|line| serde_json::from_str(line).expect("an event of the kept log"))
        .collect();
    let state = RunState::replay(&events);
    let record = state.tasks.get(TASK).expect("the design's task");
    let before = record
        .checked_before
        .as_ref()
        .expect("the task's check before the work");
    assert_eq!(before.phase, Phase::Pre);
    let (guards, own): (Vec<&CriterionResult>, Vec<&CriterionResult>) = before
        .results
        .iter()
        .partition(|result| result.r#type == Some(CriterionType::Guard));
    assert_eq!(guards.len(), GUARDS, "{guards:?}");
    assert!(
        guards.iter().all(|guard| guard.exit_code == 0),
        "{guards:?}"
    );
    assert_eq!(own.len(), 1, "{own:?}");
    assert_ne!(own[0].exit_code, 0, "the task's criterion: {own:?}");
}

/// The share, in hundredths, of each session's input read from its cache: the design's
/// numbers say `96% cached tokens per input token`.
const CACHED_HUNDREDTHS: u64 = 96;

/// The input and the output every session of the design's run spends together, as the
/// design's numbers say them: 27.1M tokens, the cache a part of the input.
const SPENT: (u64, u64) = (26_054_000, 1_064_000);

#[test]
fn every_session_of_the_kept_run_reads_its_input_from_the_cache_as_the_design_shows() {
    let events = kept();
    let usages: Vec<&Value> = events
        .iter()
        .filter(|event| event["kind"] == "agent_message" && event["message_type"] == "usage")
        .collect();
    assert!(!usages.is_empty(), "the kept log reports no usage");
    for usage in &usages {
        let input = usage["input_tokens"].as_u64().expect("a usage's input");
        assert_eq!(
            usage["cached_input_tokens"].as_u64(),
            Some(input * CACHED_HUNDREDTHS / 100),
            "seq {}: its input {input}",
            usage["seq"]
        );
    }
    let spent = |field: &str| {
        usages
            .iter()
            .map(|usage| usage[field].as_u64().expect("a usage's count"))
            .sum::<u64>()
    };
    assert_eq!((spent("input_tokens"), spent("output_tokens")), SPENT);
}

/// The seconds each kept `duration_ms` of `value` holds, in the order the event holds them.
fn durations(value: &Value, found: &mut Vec<u64>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                match (key.as_str(), field.as_u64()) {
                    ("duration_ms", Some(millis)) => found.push(millis),
                    _ => durations(field, found),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| durations(item, found)),
        _ => {}
    }
}

#[test]
fn the_failed_check_took_what_the_design_shows_and_no_duration_outlasts_its_node() {
    let events = kept();
    assert_eq!(
        worked(&events, "tests", "node_started", "node_failed"),
        32,
        "the suite that fails at `tests` took 32s"
    );
    let mut timed = 0;
    for (at_seq, event) in events.iter().enumerate() {
        let mut found = Vec::new();
        durations(event, &mut found);
        if found.is_empty() {
            continue;
        }
        timed += 1;
        let began = events[..at_seq]
            .iter()
            .rev()
            .find(|earlier| match event["node_id"].as_str() {
                Some(node) => is(earlier, "node_started", node),
                None => earlier["kind"] == "run_created" || earlier["kind"] == "run_resumed",
            })
            .expect("what an event that times a check stands in");
        let span = (at(event) - at(began)).num_milliseconds();
        for millis in found {
            assert!(
                i64::try_from(millis).is_ok_and(|millis| millis <= span),
                "seq {}: a check of {millis} ms in a span of {span} ms",
                event["seq"]
            );
        }
    }
    assert!(timed > 0, "the kept log times no check");
}
