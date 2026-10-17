//! A run's log keeps what the engine knows as data: an option keeps its id
//! and where it leads, a finding its cause, a rule its fault. Every string in
//! it with a space is something the run was handed.

mod common;
use common::*;

use serde_json::{json, Value};
use yunta_core::events::StoredEvent;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, ScriptedInteraction, MOCK_CONFIG};

/// A mock run with no sessions to play.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// Keys of a line that hold no words: the envelope, and the host's own
/// environment, which the run records as it found it (its PATH may hold a
/// directory with a space).
const BOOKKEEPING: &[&str] = &[
    "kind",
    "run_id",
    "seq",
    "timestamp",
    "node_id",
    "event_hash",
    "environment",
];

/// Paths whose text is an error's report: the words of the call that failed,
/// with its causes, which a log keeps as text by design. A rejected session's
/// message is one, since the adapter that met the refusal wrote it.
const REPORTS: &[&str] = &[".report", ".session_refused.message"];

/// Each line of `events` as `events.jsonl` writes it.
fn lines(events: &[StoredEvent]) -> Vec<Value> {
    events
        .iter()
        .map(|event| serde_json::to_value(event).expect("an event is JSON"))
        .collect()
}

/// The first line of `kind`.
fn first(events: &[StoredEvent], kind: &str) -> Value {
    lines(events)
        .into_iter()
        .find(|line| line["kind"] == kind)
        .unwrap_or_else(|| panic!("no `{kind}` on the log"))
}

/// Each string with a space that none of `given` holds, with where it sits:
/// the words the run wrote of its own.
fn engine_words(events: &[StoredEvent], given: &[&str]) -> Vec<String> {
    fn walk(value: &Value, at: &str, given: &[&str], found: &mut Vec<String>) {
        match value {
            Value::Object(fields) => fields
                .iter()
                .filter(|(key, _)| !BOOKKEEPING.contains(&key.as_str()))
                .for_each(|(key, inner)| walk(inner, &format!("{at}.{key}"), given, found)),
            Value::Array(items) => items
                .iter()
                .for_each(|inner| walk(inner, &format!("{at}[]"), given, found)),
            Value::String(text)
                if text.trim().contains(' ')
                    && !given.iter().any(|handed| handed.contains(text.as_str()))
                    && !REPORTS.iter().any(|suffix| at.ends_with(suffix)) =>
            {
                found.push(format!("{at}: {text}"));
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for line in lines(events) {
        let kind = line["kind"].as_str().unwrap_or("?").to_string();
        walk(&line, &kind, given, &mut found);
    }
    found
}

/// A workflow whose gate asks a person which plan to ship: one option goes
/// back to the plan, one goes on to the node that waits on the gate.
const GATED: &str = r#"
name: gate-words
nodes:
  - { id: plan, kind: bash, run: "true" }
  - id: approve
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Is this the plan to ship?"
    options: [proceed, revise]
    on: { revise: plan }
  - { id: ship, kind: bash, depends_on: [approve], run: "true" }
"#;

#[tokio::test]
async fn a_gate_answered_at_the_terminal_keeps_ids_and_where_they_lead() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["proceed"]);
    let RunReport { terminal, .. } = bench
        .run_with_interaction(GATED, NO_SESSIONS, &interaction)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let waiting = first(&events, "gate_waiting");
    assert_eq!(
        waiting["options"],
        json!([
            { "id": "proceed", "leads": { "on": { "next": ["ship"], "settles": false } } },
            { "id": "revise", "leads": { "back": { "node": "plan", "resumes": false } } },
            { "id": "abort" },
        ])
    );
    assert_eq!(waiting["gate"], json!({ "assignee": "lead" }));
    assert!(waiting.get("evidence").is_none(), "{waiting}");
    assert_eq!(engine_words(&events, &[GATED]), Vec::<String>::new());
}

#[tokio::test]
async fn a_budget_decision_keeps_its_cap_as_data() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["continue"]);
    let RunReport { terminal, .. } = bench
        .run_full(BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG, &interaction)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let waiting = first(&events, "gate_waiting");
    assert_eq!(waiting["summary"]["claim"], "tokens_spent");
    assert!(
        waiting["budget"]["cap"].is_u64() && waiting["budget"]["spent"].is_u64(),
        "{waiting}"
    );
    assert_eq!(
        waiting["options"],
        json!([{ "id": "continue" }, { "id": "abort" }])
    );
    assert_eq!(
        engine_words(&events, &[BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG]),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_loop_cap_decision_keeps_its_cap_as_data() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["continue"]);
    let fixture = loop_cap_fixture();
    let RunReport { terminal, .. } = bench
        .run_full(LOOP_CAP_WORKFLOW, &fixture, LOOP_CAP_CONFIG, &interaction)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let waiting = first(&events, "gate_waiting");
    assert_eq!(waiting["summary"]["claim"], "loop_cap_spent");
    assert!(
        waiting["loop_cap"]["cap"].is_u64() && waiting["loop_cap"]["ran"].is_u64(),
        "{waiting}"
    );
    assert_eq!(
        waiting["options"],
        json!([{ "id": "continue" }, { "id": "abort" }])
    );
    assert_eq!(
        engine_words(&events, &[LOOP_CAP_WORKFLOW, &fixture, LOOP_CAP_CONFIG]),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_scope_request_put_to_a_person_keeps_its_paths_as_data() {
    let bench = Bench::new();
    let workflow = scope_expansion_workflow("ask", None);
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.txt", "test -f a.txt && test -f b.txt")
    );
    let mut fixture = plan_session(&tasks);
    fixture.push_str(&requesting_session("task-h"));
    for _ in 0..2 {
        fixture.push_str("  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-h }\n");
    }
    let person = ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "grant".into(),
        by: "lead".into(),
        free_text: None,
    });
    let RunReport { terminal, .. } = bench
        .run_with_interaction(&workflow, &fixture, &person)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let waiting = first(&events, "gate_waiting");
    assert_eq!(waiting["summary"]["claim"], "scope_asked");
    assert_eq!(waiting["scope"]["paths"], json!(["b.txt"]));
    assert_eq!(
        waiting["options"],
        json!([{ "id": "grant" }, { "id": "deny" }])
    );
    assert_eq!(
        engine_words(&events, &[&workflow, &fixture]),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_refused_request_leaves_its_cause() {
    let bench = Bench::new();
    let workflow = no_scope_expansion_workflow();
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-d", "d", "a.txt", "test -f a.txt")
    );
    let mut fixture = plan_session(&tasks);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-d\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: did-d }}\n",
        request_step(json!({ "paths": ["b.txt"], "reason": "need an adjacent fix in b.txt", "proposed_criterion": { "cmd": "test -f b.txt" } })),
    ));
    let RunReport { terminal, .. } = bench.run(&workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let finding = &first(&events, "finding_posted")["finding"];
    assert!(
        finding.get("title").is_none() && finding.get("detail").is_none(),
        "{finding}"
    );
    assert_eq!(
        finding["cause"],
        json!({ "scope_denied": {
            "asker": { "task": "task-d" },
            "denial": { "type": "mode_deny" },
            "asked": { "paths": ["b.txt"], "reason": "need an adjacent fix in b.txt" },
        } })
    );
    assert_eq!(
        finding["proposed_criterion"],
        json!({ "cmd": "test -f b.txt" })
    );
    assert_eq!(
        engine_words(&events, &[&workflow, &fixture]),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_note_keeps_its_size_and_its_digest() {
    let bench = Bench::new();
    let fixture = "dialect: claude-code\nsessions:\n  - steps:\n      - { type: note, text: \"thinking about SECRET-TOKEN-123 carefully\" }\n    outcome: { type: completed, summary: \"done\" }\n";
    let RunReport { terminal, .. } = bench.run(SESSION_EVENTS_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let note = lines(&events)
        .into_iter()
        .find(|line| line["kind"] == "agent_message" && line["message_type"] == "note")
        .expect("the note is on the log");
    assert_eq!(
        note["note"],
        json!({ "bytes": 41, "sha256": "052f9656f4d7" })
    );
    assert!(note.get("text").is_none(), "{note}");
    assert_eq!(
        engine_words(&events, &[SESSION_EVENTS_WORKFLOW, fixture]),
        Vec::<String>::new()
    );
}

/// A workflow whose planner hands over a tasks document, which the run checks.
const PLANS: &str = "name: plans\nnodes:\n  - id: plan\n    kind: prompt\n    runner: planner\n    prompt: \"Hand over the tasks document.\"\n    artifacts:\n      produces: [tasks]\n";

#[tokio::test]
async fn a_refused_document_keeps_its_faults() {
    let bench = Bench::new();
    let fixture = plan_session(
        "tasks:\n  - id: T001\n    title: \"\"\n    scope: [\"a.txt\"]\n    criteria:\n      - cmd: \"test -f a.txt\"\n",
    );
    bench.run_with_config(PLANS, &fixture, MOCK_CONFIG).await;
    let events = bench.events();
    let refused = lines(&events)
        .into_iter()
        .find(|line| {
            line["kind"] == "artifact_submitted" && line["outcome"].get("refused").is_some()
        })
        .expect("the document is refused");
    let diagnostic = &refused["outcome"]["refused"]["report"]["diagnostics"][0];
    assert_eq!(
        (&diagnostic["problem"], &diagnostic["code"]),
        (&json!("rule"), &json!("empty-title"))
    );
    assert!(
        diagnostic.get("fault").is_some() && diagnostic.get("detail").is_none(),
        "{diagnostic}"
    );
    assert_eq!(
        engine_words(&events, &[PLANS, &fixture, MOCK_CONFIG]),
        Vec::<String>::new()
    );
}

/// A hook whose command the project's permissions deny.
const DENIED_HOOK: &str = "name: hook-violation\nnodes:\n  - id: build\n    kind: bash\n    run: \"true\"\n    hooks:\n      before:\n        - run: \"echo forbidden-marker\"\n          on_failure: warn\n";

#[tokio::test]
async fn a_denied_command_keeps_the_rule_as_data() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(DENIED_HOOK, NO_SESSIONS, CONFIG_WITH_DENY)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let events = bench.events();
    assert_eq!(
        first(&events, "node_failed")["denied_rule"],
        json!({
            "command": "echo forbidden-marker",
            "by": { "list": "deny", "pattern": "*forbidden-marker*" },
        })
    );
    assert_eq!(
        engine_words(&events, &[DENIED_HOOK, CONFIG_WITH_DENY]),
        Vec::<String>::new()
    );
}

/// A workflow whose last node opens a pull request on a forge.
const OPENS: &str = "name: opens\nnodes:\n  - { id: make, kind: bash, run: \"echo made > made.txt\" }\n  - { id: pr, kind: pull_request, depends_on: [make], title: \"add dark mode\", body: \"What the run did.\" }\n";
const FORGE: &str = "forge:\n  github: { repo: acme/web, token_env: ACME_TOKEN }\n";

#[tokio::test]
async fn a_forge_the_machine_cannot_reach_keeps_the_step_as_data() {
    let bench = Bench::new();
    let config = format!("{MOCK_CONFIG}{FORGE}");
    let RunReport { terminal, .. } = bench.run_with_config(OPENS, NO_SESSIONS, &config).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let events = bench.events();
    assert_eq!(
        first(&events, "node_failed")["engine_error"],
        json!({ "step": "forge_token" })
    );
    assert_eq!(
        engine_words(&events, &[OPENS, &config]),
        Vec::<String>::new()
    );
}
