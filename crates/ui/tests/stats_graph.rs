//! What a run cost and how a workflow is drawn, in the application: a finished run's
//! stats as `yunta stats` prints them, at a glance and with every row; its graph with
//! each node labelled with its word, the diagram `yunta graph --run` prints; and,
//! from the catalog, a workflow's past runs as `yunta stats --workflow` prints them
//! and its graph as written.

mod support;

use std::path::Path;
use std::sync::Arc;

use surco::doc::Depth;
use surco::surface::{Markdown, Surface};
use yunta_core::{Workflow, WorkflowName};
use yunta_render::graph::{self, Labels};
use yunta_render::page::{Graph, History, Page, Stats};
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::workspace;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Performer, State};

use support::{on_the_field, stopped};

/// Two nodes that finish, one after the other.
const FINISHES: &str = "name: done\nnodes:\n  - { id: first, kind: bash, run: \"true\" }\n  \
                        - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// A node an agent works, then one that finishes after it: what the agent's session
/// ran on is a row only every row of the stats shows.
const WORKED: &str =
    "name: worked\nnodes:\n  - id: first\n    kind: prompt\n    runner: executor\n    \
                      prompt: \"Say it is done.\"\n  \
                      - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// The agent's one session, which ends done.
const SAYS_DONE: &str =
    "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: done }\n";

/// The state root of `bench`'s repository: where its runs and log are.
fn root_of(bench: &Bench) -> &Path {
    bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root")
}

/// A bench whose run of `workflow` on `sessions`, under `run_id`, finished.
async fn finished(run_id: &str, workflow: &str, sessions: &str) -> Bench {
    let bench = Bench::with_run_id(run_id);
    bench.run(workflow, sessions).await;
    bench
}

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

/// `page` as Markdown writes it.
fn drawn(page: &impl Page) -> String {
    let look = Markdown.look();
    Markdown.draw(&page.document(&look))
}

#[tokio::test]
async fn a_finished_runs_stats_are_those_stats_prints_at_a_glance_and_whole() {
    let bench = finished("01M7STATS00000000000000000", WORKED, SAYS_DONE).await;
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let field = on_the_field(vec![stopped(&bench.run_id)]);
    // The run opened, then what it cost.
    let strokes = [Stroke::Enter, Stroke::Insert('s')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let project = workspace(&bench.worktree, root_of(&bench))
        .project()
        .expect("the project");
    let spent = project.run(&bench.run_id).await.expect("the run").stats();
    let printed = |depth| {
        drawn(&Stats {
            frame: &spent.frame,
            stats: &spent.stats,
            pricing: spent.pricing.as_ref(),
            depth,
        })
    };
    assert_eq!(main_region(&state), printed(Depth::Glance));
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail.last().map(String::as_str), Some("stats"));
    // Every row, as `--full` prints them, then the glance again.
    assert_ne!(
        printed(Depth::Full),
        printed(Depth::Glance),
        "every row says more"
    );
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Insert('f')]).await;
    assert_eq!(main_region(&state), printed(Depth::Full));
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Insert('f')]).await;
    assert_eq!(main_region(&state), printed(Depth::Glance));
}

#[tokio::test]
async fn a_runs_graph_labels_each_node_with_its_word_as_graph_run_prints_it() {
    let bench = finished("01M7GRAPH00000000000000000", FINISHES, NO_SESSIONS).await;
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let field = on_the_field(vec![stopped(&bench.run_id)]);
    let strokes = [Stroke::Enter, Stroke::Insert('g')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let project = workspace(&bench.worktree, root_of(&bench))
        .project()
        .expect("the project");
    let open = project.run(&bench.run_id).await.expect("the run");
    let (workflow, frame) = (open.manifest.doc.workflow.clone(), open.frame());
    let shown = main_region(&state);
    // What `yunta graph --run` prints is the diagram the stage draws.
    let printed = graph::mermaid(&workflow, Some(&Labels::of(&frame)));
    assert!(
        shown.contains(printed.trim_end()),
        "{shown}\n---\n{printed}"
    );
    for node in ["first: finished", "second: finished"] {
        assert!(shown.contains(node), "{node} labels its node: {shown}");
    }
    let page = Graph {
        workflow,
        run: Some((bench.run_id.clone(), frame)),
    };
    assert_eq!(shown, drawn(&page));
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail.last().map(String::as_str), Some("graph"));
}

#[tokio::test]
async fn a_workflows_past_runs_and_its_graph_open_from_the_catalog() {
    let bench = finished("01M7HISTORY000000000000000", FINISHES, NO_SESSIONS).await;
    let workflows = bench.worktree.join(".yunta/workflows");
    std::fs::create_dir_all(&workflows).expect("the catalog's directory");
    // Offered by its file's stem, its runs kept under the name the file gives it.
    std::fs::write(workflows.join("finishes.yaml"), FINISHES).expect("the workflow that ran");
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let field = on_the_field(vec![stopped(&bench.run_id)]);
    // The catalog opened, then the past runs of the workflow it points at.
    let strokes = [Stroke::Insert('w'), Stroke::Insert('h')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let project = workspace(&bench.worktree, root_of(&bench))
        .project()
        .expect("the project");
    let name = WorkflowName::from("done");
    let past = project.history(&name).await.reading();
    assert_eq!(past.summaries.len(), 1, "the run is the workflow's past");
    let printed = History {
        name: &name,
        history: &past.summaries,
        findings: past.findings.as_ref(),
    };
    assert_eq!(main_region(&state), drawn(&printed));
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail, ["yunta", "workflows", "done", "history"]);
    // Back to the workflows, then the graph of the one pointed at, as written.
    let strokes = [Stroke::Decline, Stroke::Insert('g')];
    let (state, _) = drive(state, Vec::new(), &performer, &strokes).await;
    let page = Graph {
        workflow: Workflow::parse(FINISHES).expect("the workflow reads"),
        run: None,
    };
    assert_eq!(main_region(&state), drawn(&page));
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail, ["yunta", "workflows", "finishes", "graph"]);
}
