//! Every run and every node executes inside a tracing span that carries
//! the `run_id` (and, for a node, the `node_id`) — the fields an operator
//! filters logs by. A capturing subscriber reads them back off a real run.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::Attributes;
use tracing::Subscriber;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use yunta_testkit::Bench;

/// One recorded span: its name and its fields as strings.
type RecordedSpan = (String, BTreeMap<String, String>);

#[derive(Clone, Default)]
struct SpanCapture(Arc<Mutex<Vec<RecordedSpan>>>);

#[derive(Default)]
struct FieldMap(BTreeMap<String, String>);

impl Visit for FieldMap {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // A `%value` (Display) field arrives here with its raw text and no
        // surrounding quotes; a genuine Debug value keeps its own form.
        self.0
            .entry(field.name().to_string())
            .or_insert_with(|| format!("{value:?}"));
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for SpanCapture {
    fn on_new_span(&self, attrs: &Attributes<'_>, _id: &tracing::span::Id, _ctx: Context<'_, S>) {
        let mut fields = FieldMap::default();
        attrs.record(&mut fields);
        self.0
            .lock()
            .unwrap()
            .push((attrs.metadata().name().to_string(), fields.0));
    }
}

#[tokio::test]
async fn node_execution_runs_inside_a_span_carrying_run_id_and_node_id() {
    let capture = SpanCapture::default();
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(capture.clone()));

    let workflow = "name: spans\nnodes:\n  - id: build\n    kind: bash\n    run: \"true\"\n";
    Bench::with_run_id("run-spans")
        .run_with_config(workflow, "sessions: []\n", "runners: {}\n")
        .await;

    let spans = capture.0.lock().unwrap();
    let node_span = spans
        .iter()
        .find(|(name, _)| name == "execute_node")
        .expect("an execute_node span was recorded");
    assert_eq!(
        node_span.1.get("node_id").map(String::as_str),
        Some("build"),
        "the node span names its node_id: {:?}",
        node_span.1
    );
    assert_eq!(
        node_span.1.get("run_id").map(String::as_str),
        Some("run-spans"),
        "and its run_id: {:?}",
        node_span.1
    );
    assert!(
        spans
            .iter()
            .any(|(name, fields)| name == "execute_run_at_depth"
                && fields.get("run_id").map(String::as_str) == Some("run-spans")),
        "the run itself has a run_id span"
    );
}

/// A gate and a questions node are the two places a run stops for a
/// person, and the two an operator reaches for first when one is stuck.
/// Each executes inside its own span naming which run and which node.
///
/// Two runs, because each of these parks its own run: a workflow with
/// both would only ever reach the first.
#[tokio::test]
async fn every_gate_and_questions_node_carries_a_node_span() {
    let capture = SpanCapture::default();
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(capture.clone()));

    let asking = r#"
name: asks
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Ask what you need to know before continuing."
    artifacts:
      produces: [questions]
"#;
    let asked = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - id: q1
                text: "Which environment?"
                answer_type: text
                required: true
    outcome: { type: completed, summary: "asked" }
"#;
    Bench::new().run(asking, asked).await;

    let gating = r#"
name: gates
nodes:
  - id: approve
    kind: gate
    assignee: lead
"#;
    Bench::new().run(gating, "sessions: []\n").await;

    let spans = capture.0.lock().unwrap();
    let recorded: Vec<&str> = spans.iter().map(|(name, _)| name.as_str()).collect();
    for name in ["execute_ask", "resolve_internal_gate"] {
        let span = spans
            .iter()
            .find(|(seen, _)| seen == name)
            .unwrap_or_else(|| panic!("`{name}` runs inside a span of its own: {recorded:?}"));
        assert!(
            span.1.contains_key("run_id") && span.1.contains_key("node_id"),
            "`{name}`'s span names the run and the node: {:?}",
            span.1
        );
    }
}
