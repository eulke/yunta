//! A workflow's graph as a page, in every environment: as its file declares it,
//! and as a run of it draws it, headed by its mode, each node a box holding its id
//! alone. A group is drawn as a box holding its nodes, the links to what it holds
//! crossing its edge; a graph with none is drawn as its chain.

use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::{ModeName, Workflow};

use super::Graph;
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

/// The mode a run of the sample workflows was created in.
fn standard() -> ModeName {
    ModeName::from("standard")
}

/// A run's graph is headed by the nodes its workflow runs, in order, with the run's mode
/// at the far end, and its workflow as written by the same words with `as written` there;
/// each node's box holds its id alone.
#[test]
fn a_runs_graph_is_headed_by_the_nodes_it_runs_and_labels_each_node_by_its_id() {
    let look = Look::plain();
    let run = Graph {
        workflow: workflow(),
        mode: Some(standard()),
    };
    let drawn = Terminal::on(look).draw(&run.document(&look));
    let headline = drawn.lines().next().expect("a headline");
    assert!(
        headline.starts_with("> checked ; the nodes it runs, in order "),
        "{drawn}"
    );
    assert!(headline.ends_with(" standard"), "{drawn}");
    assert_eq!(headline.len(), look.width.cells(), "{drawn}");
    for id in ["build", "lint", "test", "fix", "ship"] {
        assert!(drawn.contains(&format!("| {id} |")), "{drawn}");
    }
    let as_written = Graph {
        workflow: workflow(),
        mode: None,
    };
    let drawn = Terminal::on(look).draw(&as_written.document(&look));
    let headline = drawn.lines().next().expect("a headline");
    assert!(
        headline.starts_with(". checked ; the nodes it runs, in order "),
        "{drawn}"
    );
    assert!(headline.ends_with(" as written"), "{drawn}");
}

/// The workflow the design's run froze in its manifest.
fn designed() -> Workflow {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../testkit-ui/fixtures/designed/run/manifest.yaml");
    let frozen = std::fs::read(path).expect("the design's run is kept");
    yunta_core::Manifest::read_frozen(&frozen)
        .expect("the design's manifest reads")
        .doc
        .workflow
}

/// The graph of the design's run, in `standard`, the mode its run was created in, reads as
/// its targets draw it at the width each target's region takes, the whole diagram under
/// its headline.
#[test]
fn the_design_s_graph_reads_as_its_targets_draw_it() {
    let page = Graph {
        workflow: designed(),
        mode: Some(standard()),
    };
    for (size, glyphs, cells) in [
        ("unicode-120", surco::glyphs::Glyphs::Unicode, 118),
        ("ascii-120", surco::glyphs::Glyphs::Ascii, 118),
        ("unicode-80", surco::glyphs::Glyphs::Unicode, 78),
        ("unicode-60", surco::glyphs::Glyphs::Unicode, 58),
    ] {
        let look = Look {
            glyphs,
            paint: surco::paint::Paint::Plain,
            width: surco::look::Width::exact(cells),
            motion: surco::motion::Motion::Moves,
        };
        assert_golden(
            &goldens().join(format!("designed.{size}.txt")),
            &Terminal::on(look).draw(&page.document(&look)),
        );
    }
}

#[test]
fn a_workflows_graph_matches_its_goldens_in_every_environment() {
    let pages = [
        (
            "as-written",
            Graph {
                workflow: workflow(),
                mode: None,
            },
        ),
        (
            "chained",
            Graph {
                workflow: chain(),
                mode: None,
            },
        ),
        (
            "of-a-run",
            Graph {
                workflow: workflow(),
                mode: Some(standard()),
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
