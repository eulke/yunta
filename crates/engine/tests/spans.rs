//! Every run and every node executes inside a tracing span that carries
//! the `run_id` (and, for a node, the `node_id`) — the fields an operator
//! filters logs by. A capturing subscriber reads them back off a real run.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::Attributes;
use tracing::Subscriber;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use yunta_core::{ConfigLayer, RunId, SeqIdSource, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, RunEnv,
    DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit::{init_repo, FixedClock};

static IDS: SeqIdSource = SeqIdSource::new("spans");

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

    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let runs_root = root.path().join("runs");
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    let run_id = RunId::from("run-spans");

    let workflow: Workflow = serde_norway::from_str(
        "name: spans\nnodes:\n  - id: build\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    let config: ConfigLayer = serde_norway::from_str("runners: {}\n").unwrap();
    let manifest =
        build_manifest(&workflow, &config, &worktree, &worktree, &HashMap::new()).unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    execute_run(RunEnv {
        run_id: &run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &HashMap::new(),
        storage: &storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
        observer: None,
    })
    .await
    .unwrap();

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
