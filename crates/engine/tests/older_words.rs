//! Lines an earlier binary wrote, one of each shape that stored words where
//! the log now holds data. Each line reads, a surface shows its words as they
//! were stored, and writing it back gives the same line. The first five are
//! lines a run wrote, with the run's id replaced; the other four are built in
//! the shape their fields had.

use serde_json::{json, Value};
use yunta_core::events::{
    ArtifactEvent, EventPayload, Failure, FindingEvent, GateEvent, GateWaitingPayload, LoggedWords,
    NodeEvent, NodeFailedPayload, SessionEvent, StoredEvent, SubmissionOutcome,
};
use yunta_render::{escalation, failure, found, offers};

/// The run every built line belongs to.
const RUN: &str = "01R0000000000000000000000H";

/// The keys of a line's envelope. A line writes them in its own order, and its
/// timestamp with its own offset, so a line is compared by its body alone.
const ENVELOPE: &[&str] = &["run_id", "seq", "timestamp", "node_id", "schema_version"];

/// A `gate_waiting` line whose options store the label and trade-off they show.
const GATE_OPTIONS: &str = r##"{"evidence":[{"label":"paths","value":"crates/cli/Cargo.toml"},{"value":"proposed criterion `cargo test -p yunta --lib -- --list | grep -q 'terminal_color_policy_uses_the_shared_palette_only_on_supported_output' && cargo test -p yunta --lib terminal_color_policy_uses_the_shared_palette_only_on_supported_output` pre-check exit: 1"},{"label":"mode","value":"ask"},{"value":"0/3 grant(s) used"}],"kind":"gate_waiting","node_id":"implement","options":[{"id":"grant","label":"Grant access to crates/cli/Cargo.toml","tradeoff":"The task's final diff is evaluated against its scope plus these paths; consumes 1 of max_per_run"},{"id":"deny","label":"Deny the request","tradeoff":"The denial becomes a finding; the task retries within its original scope"}],"run_id":"01R0000000000000000000000H","seq":88,"summary":"[agent prose elided]","timestamp":"2026-09-27T16:53:46.441617Z"}"##;

/// A `finding_posted` line whose finding a session wrote in words.
const FINDING_WORDS: &str = r##"{"seq": 1872, "timestamp": "2026-10-15T05:54:31.127510+00:00", "run_id": "01R0000000000000000000000H", "node_id": "implement", "kind": "finding_posted", "finding": {"id": "scope-expansion-resolve-global-1", "severity": "minor", "title": "scope expansion denied for task `resolve-global`", "location": "crates/cli/src/commands/doctor/mod.rs", "detail": "proposed criterion already passes \u2014 nothing to expand for \u2014 paths asked for: crates/cli/src/commands/doctor/mod.rs, crates/engine/src/run/session_plan.rs; agent's stated reason: doctor/mod.rs builds `layers` (PackLayers) at line 60 but still passes `&ctx.cwd` to `check_workflow_refs` at line 386. Fixing this one-line call site is required to thread PackLayers through all callers of the updated check_workflow_refs signature.", "proposed_criterion": {"cmd": "cargo check --workspace 2>&1 | grep -c 'error\\[E'"}}}"##;

/// An `agent_message` line whose note stored its words as its text.
const NOTE_TEXT: &str = r##"{"seq": 7, "timestamp": "2026-10-14T23:40:19.800873+00:00", "run_id": "01R0000000000000000000000H", "node_id": "grill", "kind": "agent_message", "message_type": "note", "text": "135 bytes, sha256 2eb6d565235b"}"##;

/// An `artifact_submitted` line whose refused document stored the clause of
/// each rule it broke.
const RULE_CLAUSES: &str = r##"{"seq": 196, "timestamp": "2026-10-15T00:14:29.290874+00:00", "run_id": "01R0000000000000000000000H", "node_id": "plan", "kind": "artifact_submitted", "name": "tasks.yaml", "artifact_kind": "tasks", "outcome": {"refused": {"report": {"document": {"kind": "tasks", "path": "artifacts/plan/tasks.yaml"}, "diagnostics": [{"of": "task", "id": "resolve-global", "index": 3, "problem": "rule", "code": "change-outside-scope", "detail": "changes `crates/engine/src/frozen/walk.rs`, which its scope does not cover; widen the scope, or leave the change to the task that owns the file"}, {"of": "task", "id": "resolve-global", "index": 3, "problem": "rule", "code": "change-outside-scope", "detail": "changes `crates/engine/src/frozen/provenance.rs`, which its scope does not cover; widen the scope, or leave the change to the task that owns the file"}, {"of": "task", "id": "resolve-global", "index": 3, "problem": "rule", "code": "change-outside-scope", "detail": "changes `crates/engine/src/check/refs.rs`, which its scope does not cover; widen the scope, or leave the change to the task that owns the file"}, {"of": "task", "id": "resolve-global", "index": 3, "problem": "rule", "code": "change-outside-scope", "detail": "changes `crates/engine/src/check/packs.rs`, which its scope does not cover; widen the scope, or leave the change to the task that owns the file"}, {"of": "task", "id": "resolve-global", "index": 3, "problem": "rule", "code": "change-outside-scope", "detail": "changes `crates/engine/src/run/workflow_exec/children.rs`, which its scope does not cover; widen the scope, or leave the change to the task that owns the file"}, {"of": "task", "id": "catalog-global-view", "index": 4, "problem": "rule", "code": "change-outside-scope", "detail": "changes `crates/cli/src/commands/list/mod.rs`, which its scope does not cover; widen the scope, or leave the change to the task that owns the file"}, {"of": "task", "id": "catalog-global-view", "index": 4, "problem": "rule", "code": "change-outside-scope", "detail": "changes `crates/cli/src/commands/doctor/mod.rs`, which its scope does not cover; widen the scope, or leave the change to the task that owns the file"}]}}}}"##;

/// A `node_failed` line whose owed scope stored the reason its session gave.
const OWED_REASON: &str = r##"{"seq": 1179, "timestamp": "2026-10-15T03:11:09.092338+00:00", "run_id": "01R0000000000000000000000H", "node_id": "implement", "kind": "node_failed", "owed": [{"task_id": "resolve-global", "paths": ["crates/engine/src/catalog.rs"], "reason": "PackLayers in catalog.rs needs three additions for the resolve-global task: (1) remove #[expect(dead_code)] from pack_dir and scope_of since callers are being added in this task, (2) add pub(crate) fn repo_root(&self) -> &Path accessor needed by agents.rs and skills.rs, and (3) add pub(crate) fn lock_path(&self, scope: PackScope) -> PathBuf method needed by provenance.rs. Without these, the call chain from provenance/walk/agents/skills cannot compile."}], "tokens_used": {"input": 13020941, "output": 69775, "cached": 12047942}, "tree": "23faf1d2a13d0b817db76fc5b9ca9f22d3957060"}"##;

/// Checks that the words of one older line come out of render as it stored them.
type Shows = fn(&StoredEvent, &Value);

/// Each older shape, with the check its words are shown by.
fn older_lines() -> Vec<(&'static str, Value, Shows)> {
    vec![
        (
            "gate option words",
            copied(GATE_OPTIONS),
            gate_option_words as Shows,
        ),
        (
            "finding words",
            copied(FINDING_WORDS),
            finding_words as Shows,
        ),
        ("note text", copied(NOTE_TEXT), note_text as Shows),
        ("rule clause", copied(RULE_CLAUSES), rule_clause as Shows),
        ("owed reason", copied(OWED_REASON), owed_reason as Shows),
        (
            "engine error sentence",
            engine_error_sentence(),
            engine_error_words as Shows,
        ),
        (
            "denied rule sentence",
            denied_rule_sentence(),
            denied_rule_words as Shows,
        ),
        (
            "session not stopped sentence",
            session_not_stopped_sentence(),
            session_not_stopped_words as Shows,
        ),
        (
            "withheld reason",
            withheld_reason(),
            withheld_words as Shows,
        ),
    ]
}

#[test]
fn every_older_word_reads_shows_and_writes_back() {
    for (name, line, shows) in older_lines() {
        let event: StoredEvent = serde_json::from_value(line.clone())
            .unwrap_or_else(|error| panic!("the {name} line reads: {error}"));
        shows(&event, &line);
        let written = serde_json::to_value(&event).expect("an event writes back");
        assert_eq!(
            body(&written),
            body(&line),
            "the {name} line writes back as it was read"
        );
    }
}

/// A line's body, without its envelope.
fn body(line: &Value) -> Value {
    let mut body = line.clone();
    if let Some(fields) = body.as_object_mut() {
        for key in ENVELOPE {
            fields.remove(*key);
        }
    }
    body
}

/// A line copied from a log, as JSON.
fn copied(text: &str) -> Value {
    serde_json::from_str(text).expect("a line of a log is JSON")
}

/// The payload of a `gate_waiting` line.
fn waiting(event: &StoredEvent) -> &GateWaitingPayload {
    match event.payload() {
        Some(EventPayload::Gates(GateEvent::Waiting(payload))) => payload,
        other => panic!("a gate_waiting line, not {other:?}"),
    }
}

/// The payload of a `node_failed` line.
fn failed(event: &StoredEvent) -> &NodeFailedPayload {
    match event.payload() {
        Some(EventPayload::Node(NodeEvent::Failed(payload))) => payload,
        other => panic!("a node_failed line, not {other:?}"),
    }
}

/// Each option shows the label and the trade-off its line stored, and the
/// facts beside the claim show as the line stored them.
fn gate_option_words(event: &StoredEvent, line: &Value) {
    let payload = waiting(event);
    let stored = line["options"].as_array().expect("the options are listed");
    assert_eq!(payload.options().len(), stored.len());
    for (option, words) in payload.options().iter().zip(stored) {
        let (label, tradeoff) = offers::option_words(payload, option);
        assert_eq!(label, words["label"].as_str().expect("a stored label"));
        assert_eq!(tradeoff.as_deref(), words["tradeoff"].as_str());
    }
    assert_eq!(
        escalation::evidence(payload),
        [
            "paths: crates/cli/Cargo.toml",
            "proposed criterion `cargo test -p yunta --lib -- --list | grep -q 'terminal_color_policy_uses_the_shared_palette_only_on_supported_output' && cargo test -p yunta --lib terminal_color_policy_uses_the_shared_palette_only_on_supported_output` pre-check exit: 1",
            "mode: ask",
            "0/3 grant(s) used",
        ]
    );
}

/// A finding a session wrote shows the title and the detail it stored.
fn finding_words(event: &StoredEvent, line: &Value) {
    let Some(EventPayload::Findings(FindingEvent::Posted(posted))) = event.payload() else {
        panic!("a finding_posted line");
    };
    let (title, detail) = found::said(&posted.finding.says);
    assert_eq!(
        title,
        line["finding"]["title"].as_str().expect("a stored title")
    );
    assert_eq!(
        detail,
        line["finding"]["detail"].as_str().expect("a stored detail")
    );
}

/// A note keeps the words it stored as its text, and no digest of them.
fn note_text(event: &StoredEvent, line: &Value) {
    let Some(EventPayload::Session(SessionEvent::Message(message))) = event.payload() else {
        panic!("an agent_message line");
    };
    assert_eq!(
        message.text.as_ref().map(LoggedWords::as_str),
        line["text"].as_str()
    );
    assert_eq!(message.note, None);
}

/// A refused document shows each rule's clause as it stored it, under the
/// task the rule broke.
fn rule_clause(event: &StoredEvent, line: &Value) {
    let Some(EventPayload::Artifacts(ArtifactEvent::Submitted(submitted))) = event.payload() else {
        panic!("an artifact_submitted line");
    };
    let SubmissionOutcome::Refused { report } = &submitted.outcome else {
        panic!("a refused document");
    };
    let clause = line["outcome"]["refused"]["report"]["diagnostics"][0]["detail"]
        .as_str()
        .expect("a stored clause");
    assert_eq!(
        report.diagnostics[0].to_string(),
        format!("task `resolve-global`: {clause}")
    );
}

/// An owed scope shows the reason its session stored, beside its paths.
fn owed_reason(event: &StoredEvent, line: &Value) {
    let node_failure = &failed(event).failure;
    let reason = line["owed"][0]["reason"].as_str().expect("a stored reason");
    assert!(
        matches!(node_failure, Failure::ScopeOwed { owed } if owed[0].reason.as_deref() == Some(reason)),
        "{node_failure:?}"
    );
    assert_eq!(
        failure::detail(node_failure),
        [format!(
            "`resolve-global` — crates/engine/src/catalog.rs: {reason}"
        )]
    );
}

/// A failed step whose sentence a log stored: the sentence is shown as written.
fn engine_error_words(event: &StoredEvent, line: &Value) {
    let stored = line["engine_error"].as_str().expect("a stored sentence");
    assert_eq!(failure::sentence(&failed(event).failure), stored);
}

/// A denied command whose sentence a log stored: the sentence is shown as written.
fn denied_rule_words(event: &StoredEvent, line: &Value) {
    let stored = line["denied_rule"].as_str().expect("a stored sentence");
    assert_eq!(failure::sentence(&failed(event).failure), stored);
}

/// A session that could not be stopped, whose sentence a log stored: render
/// adds its own lead-in, as it always has.
fn session_not_stopped_words(event: &StoredEvent, line: &Value) {
    let stored = line["session_not_stopped"]
        .as_str()
        .expect("a stored sentence");
    assert_eq!(
        failure::sentence(&failed(event).failure),
        format!("the engine could not stop the session: {stored}")
    );
}

/// A withheld option whose reason a log stored: the reason and the fact it
/// wrote beside the claim are shown as written.
fn withheld_words(event: &StoredEvent, line: &Value) {
    let payload = waiting(event);
    let stored = line["withheld"][0]["because"]
        .as_str()
        .expect("a stored reason");
    let because: Vec<String> = payload.withheld().iter().map(escalation::because).collect();
    assert_eq!(because, [stored]);
    assert_eq!(
        escalation::evidence(payload),
        [format!("withheld: `approve` — {stored}")]
    );
}

/// A `node_failed` line whose failed push stored its sentence.
fn engine_error_sentence() -> Value {
    json!({
        "run_id": RUN,
        "seq": 3,
        "timestamp": "2026-10-14T23:40:19.800873Z",
        "node_id": "publish",
        "kind": "node_failed",
        "engine_error": "pushing `yunta/run/x` to `origin`: rejected",
        "tokens_used": { "input": 0, "output": 0 },
    })
}

/// A `node_failed` line whose refused command stored the rule it broke.
fn denied_rule_sentence() -> Value {
    json!({
        "run_id": RUN,
        "seq": 4,
        "timestamp": "2026-10-14T23:40:20.100000Z",
        "node_id": "build",
        "kind": "node_failed",
        "denied_rule": "command `rm -rf x` matches denied pattern `rm *` (permissions.commands.deny)",
        "tokens_used": { "input": 0, "output": 0 },
    })
}

/// A `node_failed` line whose session that could not be stopped stored the
/// sentence of both failures.
fn session_not_stopped_sentence() -> Value {
    json!({
        "run_id": RUN,
        "seq": 5,
        "timestamp": "2026-10-14T23:40:21.100000Z",
        "node_id": "build",
        "kind": "node_failed",
        "session_not_stopped": "kill failed; taking back what the session set up failed too: busy",
        "tokens_used": { "input": 0, "output": 0 },
    })
}

/// A `gate_waiting` line whose withheld option stored its reason, and the
/// fact that reason was written as beside the claim.
fn withheld_reason() -> Value {
    json!({
        "run_id": RUN,
        "seq": 6,
        "timestamp": "2026-10-14T23:40:22.100000Z",
        "node_id": "approve",
        "kind": "gate_waiting",
        "summary": "Approve the plan?",
        "evidence": [{
            "label": "withheld",
            "value": "`approve` — the plan cannot be proven as it is written: the spec's test of `T001` runs none of the files the spec wrote",
        }],
        "options": [{ "id": "proceed" }],
        "withheld": [{
            "option": "approve",
            "because": "the plan cannot be proven as it is written: the spec's test of `T001` runs none of the files the spec wrote",
        }],
    })
}
