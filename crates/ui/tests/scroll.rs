//! A page taller than what a surface shows of it scrolls, on every stage that shows one:
//! a terminal shows each region in a few rows and hands back what it laid out, and the
//! keys move what it shows next from there. A run's graph, what it cost, its receipt,
//! its page beside its nodes and a workflow checked in the catalog's place each scroll; a page read again keeps the line it was
//! scrolled to, and another page starts at its first; a list keeps the row pointed at in
//! sight.
//!
//! Reached by: `crates/ui/src/stage.rs`, `crates/ui/src/stage/run.rs`,
//! `crates/ui/src/stage/evidence.rs`, `crates/ui/src/stage/stats.rs`,
//! `crates/ui/src/stage/graph.rs`, `crates/ui/src/stage/field.rs`,
//! `crates/surco/src/surface/terminal/scrolled.rs`

mod support;

use std::path::Path;
use std::sync::Arc;

use surco::components::{Region, Screen, ScreenLaid};
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_api::RunNews;
use yunta_core::RunId;
use yunta_testkit::Bench;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Performer, Request, State};

use support::{drive, on_the_field, performed, stopped, workspace};

/// A node an agent works, then one that finishes after it.
const WORKED: &str =
    "name: worked\nnodes:\n  - id: first\n    kind: prompt\n    runner: executor\n    \
                      prompt: \"Say it is done.\"\n  \
                      - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// The agent's one session, which ends done.
const SAYS_DONE: &str =
    "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: done }\n";

/// The rows the terminal gives each region: fewer than a page down from the first line
/// of any of these pages reaches.
const ROWS: usize = 4;

/// The lines a page moves by: a page's rows, less the row that counts what is below.
const A_PAGE: usize = ROWS - 1;

/// The terminal each region is shown on: 80 cells, in Unicode glyphs, unpainted.
fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(80),
        motion: Motion::Still,
    }
}

/// What the terminal shows of `region` in [`ROWS`] rows, each row's text.
fn shown(region: &Region) -> Vec<String> {
    let (rows, _) = Terminal::on(look()).scrolled(&region.doc, region.sight, ROWS);
    rows.iter().map(|row| row.text()).collect()
}

/// `state`'s screen shown on the terminal, each region in [`ROWS`] rows, and what was
/// laid out of each handed back as the terminal hands it back.
fn laid_out(state: &mut State) -> Screen {
    let screen = state.screen(&look());
    let terminal = Terminal::on(look());
    let main = terminal
        .scrolled(&screen.main.doc, screen.main.sight, ROWS)
        .1;
    let side = screen.side.as_ref().map(|side| {
        terminal
            .scrolled(&side.region.doc, side.region.sight, ROWS)
            .1
    });
    let overlay = None;
    state.update(Input::Laid(ScreenLaid {
        main,
        side,
        overlay,
    }));
    state.screen(&look())
}

/// `state` handed `stroke`, then shown again.
fn pressed(state: &mut State, stroke: Stroke) -> Screen {
    state.update(Input::Stroke(stroke));
    laid_out(state)
}

/// The region a stage scrolls: the page beside a run's nodes, the main one elsewhere.
fn scrolled(screen: &Screen) -> &Region {
    match &screen.side {
        Some(side) => &side.region,
        None => &screen.main,
    }
}

/// Whether the legend offers the keys that scroll a page.
fn offers_paging(screen: &Screen) -> bool {
    screen
        .legend
        .0
        .iter()
        .any(|binding| binding.does == "by the page" || binding.does == "scroll its page")
}

/// The state root of `bench`'s repository: where its runs and log are.
fn root_of(bench: &Bench) -> &Path {
    bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root")
}

/// A bench whose run of the worked workflow, under `run_id`, finished.
async fn finished(run_id: &str) -> Bench {
    let bench = Bench::with_run_id(run_id);
    bench.run(WORKED, SAYS_DONE).await;
    // The workflow the run ran, in the catalog too, for the workflows' pages to check.
    let workflows = bench.worktree.join(".yunta/workflows");
    std::fs::create_dir_all(&workflows).expect("the catalog's directory");
    std::fs::write(workflows.join("worked.yaml"), WORKED).expect("the workflow");
    bench
}

/// The application at the stage `strokes` reach from the field, over `bench`'s finished
/// run.
async fn at(bench: &Bench, strokes: &[Stroke]) -> State {
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(bench))));
    let field = on_the_field(vec![stopped(&bench.run_id)]);
    let (state, surfaced) = drive(field, Vec::new(), &performer, strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    state
}

#[tokio::test]
async fn every_page_a_stage_shows_scrolls_by_what_a_surface_laid_out_of_it() {
    let bench = finished("01M7SCROLL0000000000000000").await;
    let stages = [
        ("its page beside its nodes", vec![Stroke::Enter]),
        ("what it cost", vec![Stroke::Enter, Stroke::Insert('s')]),
        ("its graph", vec![Stroke::Enter, Stroke::Insert('g')]),
        ("its receipt", vec![Stroke::Enter, Stroke::Insert('e')]),
        (
            "a workflow checked",
            vec![Stroke::Insert('w'), Stroke::Insert('c')],
        ),
    ];
    for (what, strokes) in stages {
        let mut state = at(&bench, &strokes).await;
        let before = state.screen(&look());
        assert!(!offers_paging(&before), "{what}: nothing laid out yet");
        let whole = Terminal::on(look()).lines(&scrolled(&before).doc);
        assert!(
            whole.len() > ROWS + A_PAGE,
            "{what} is a page taller than its rows"
        );
        let screen = laid_out(&mut state);
        assert!(offers_paging(&screen), "{what}: the legend offers the keys");
        assert_eq!(scrolled(&screen).sight.top, 0, "{what}");

        let screen = pressed(&mut state, Stroke::PageDown);
        assert_eq!(scrolled(&screen).sight.top, A_PAGE, "{what}: a page down");
        let first = shown(scrolled(&screen)).remove(0);
        assert_eq!(first, whole[A_PAGE].text(), "{what}: shown from there");

        let most = whole.len() - ROWS;
        let screen = pressed(&mut state, Stroke::End);
        assert_eq!(scrolled(&screen).sight.top, most, "{what}: the last lines");
        let screen = pressed(&mut state, Stroke::PageDown);
        assert_eq!(scrolled(&screen).sight.top, most, "{what}: never past them");
        let screen = pressed(&mut state, Stroke::Home);
        assert_eq!(scrolled(&screen).sight.top, 0, "{what}: the first line");
    }
}

#[tokio::test]
async fn a_page_read_again_keeps_its_line_and_another_page_starts_at_its_first() {
    let bench = finished("01M7SCROLLAGAIN00000000000").await;
    let mut state = at(&bench, &[Stroke::Enter]).await;
    laid_out(&mut state);
    let screen = pressed(&mut state, Stroke::PageDown);
    assert_eq!(scrolled(&screen).sight.top, A_PAGE);
    // What it cost, then back: the run's page is read again, and stands where it was.
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let strokes = [Stroke::Insert('s'), Stroke::Decline];
    let (mut state, _) = drive(state, Vec::new(), &performer, &strokes).await;
    let screen = laid_out(&mut state);
    assert_eq!(scrolled(&screen).sight.top, A_PAGE, "read again, kept");
    // A node's page stands in its place, from its first line.
    let (mut state, _) = drive(state, Vec::new(), &performer, &[Stroke::Enter]).await;
    let screen = laid_out(&mut state);
    assert_eq!(screen.trail.last().map(String::as_str), Some("first"));
    assert_eq!(
        scrolled(&screen).sight.top,
        0,
        "another page, from its first line"
    );
}

#[tokio::test]
async fn a_successors_page_starts_at_its_first_line() {
    let promoted = finished("01M7SCROLLPROMOTED00000000").await;
    let successor = finished("01M7SCROLLSUCCESSOR0000000").await;
    let field = on_the_field(vec![stopped(&promoted.run_id)]);
    let mut state = field;
    // The run opened, its following and its page handed back as the performer hands them.
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let following = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::Follow { .. }) => Some(*ticket),
        _ => None,
    });
    let following = following.expect("the run is followed");
    for request in asked {
        let Request::Perform(ticket, operation) = request else {
            continue;
        };
        let (news, outcome) = performed(&promoted, operation).await;
        for news in news.into_iter().take(1) {
            state.update(Input::News(ticket, news));
        }
        if ticket != following {
            state.update(Input::Outcome(ticket, outcome));
        }
    }
    laid_out(&mut state);
    let screen = pressed(&mut state, Stroke::PageDown);
    assert_eq!(scrolled(&screen).sight.top, A_PAGE);
    // The work goes on in the successor: its page is read, and stands from its first line.
    let read = Operation::Follow {
        run: successor.run_id.clone(),
    };
    let (news, _) = performed(&successor, read).await;
    let Some(News::Following(followed)) = news.into_iter().next() else {
        panic!("a following reads the run first");
    };
    let asked = state.update(Input::News(
        following,
        News::Run(RunNews::Successor(followed)),
    ));
    assert!(
        asked.iter().any(|request| matches!(
            request,
            Request::Perform(_, Operation::ReadPage { run }) if *run == successor.run_id
        )),
        "{asked:?}"
    );
    let screen = laid_out(&mut state);
    assert_eq!(scrolled(&screen).sight.top, 0, "the successor's page");
}

#[test]
fn a_field_taller_than_its_region_keeps_the_run_pointed_at_in_sight() {
    let runs: Vec<RunId> = (0..12)
        .map(|at| format!("01M7FIELD{at:017}").parse().expect("a run's id"))
        .collect();
    let mut state = on_the_field(runs.iter().map(stopped).collect());
    laid_out(&mut state);
    for _ in 0..10 {
        state.update(Input::Stroke(Stroke::Insert('j')));
    }
    let screen = laid_out(&mut state);
    let pointer = Glyphs::Unicode.pointer();
    let rows = shown(&screen.main);
    let pointed = rows
        .iter()
        .find(|row| row.trim_start().starts_with(pointer))
        .unwrap_or_else(|| panic!("the row pointed at is in sight: {rows:?}"));
    assert!(pointed.contains(runs[10].handle()), "{pointed}");
}
