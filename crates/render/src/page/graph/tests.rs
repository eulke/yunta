//! A workflow's graph as a page, in every environment: as its file declares it,
//! and as a run that failed on it leaves it. A group is drawn as a box holding
//! its nodes, the links to what it holds crossing its edge; a graph with none is
//! drawn as its chain.

use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use surco::words::Standing;
use yunta_core::Workflow;
use yunta_replay::frame::{RunFrame, RunPhase};

use super::Graph;
use crate::page::run::fixtures::{ahead, failed_lint, finished, RUN};
use crate::page::Page;

/// A build, a group that checks two ways at once, a fix the lint reroutes to, and a
/// ship that waits on the checks.
const WORKFLOW: &str = r#"
name: checked
nodes:
  - id: build
    kind: bash
    run: "true"
  - id: checks
    kind: parallel
    depends_on: [build]
    nodes:
      - id: lint
        kind: bash
        run: "true"
        on_failure: { goto: fix, max_reroutes: 1 }
      - id: test
        kind: bash
        run: "true"
  - id: fix
    kind: bash
    run: "true"
  - id: ship
    kind: bash
    depends_on: [checks]
    run: "true"
"#;

/// A build, a lint the fix reroutes from, and a ship: no group.
const CHAIN: &str = r#"
name: chained
nodes:
  - id: build
    kind: bash
    run: "true"
  - id: lint
    kind: bash
    depends_on: [build]
    run: "true"
    on_failure: { goto: fix, max_reroutes: 1 }
  - id: fix
    kind: bash
    run: "true"
  - id: ship
    kind: bash
    depends_on: [lint]
    run: "true"
"#;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/graph")
}

fn workflow() -> Workflow {
    Workflow::parse(WORKFLOW).expect("the sample workflow parses")
}

fn chain() -> Workflow {
    Workflow::parse(CHAIN).expect("the chained workflow parses")
}

/// The run that failed on `lint`: built, the checks failed, the rest ahead.
fn failed_run() -> RunFrame {
    RunFrame {
        standing: Standing::Failed,
        phase: RunPhase::Failed { failure: None },
        nodes: vec![
            finished("build"),
            ahead("checks"),
            failed_lint(),
            ahead("test"),
            ahead("fix"),
            ahead("ship"),
        ],
        ..RunFrame::sample(&RUN)
    }
}

#[test]
fn a_workflows_graph_matches_its_goldens_in_every_environment() {
    let pages = [
        (
            "as-written",
            Graph {
                workflow: workflow(),
                run: None,
            },
        ),
        (
            "chained",
            Graph {
                workflow: chain(),
                run: None,
            },
        ),
        (
            "of-a-run",
            Graph {
                workflow: workflow(),
                run: Some((RUN, failed_run())),
            },
        ),
    ];
    for (name, page) in &pages {
        for environment in &ENVIRONMENTS {
            let look = Look::of(environment);
            let drawn = Terminal::on(look).draw(&page.document(&look));
            assert_golden(&environment.golden(&goldens(), name), &drawn);
        }
    }
}
