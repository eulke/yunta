//! A page taller than what a surface shows of it scrolls, on every stage that shows one:
//! a terminal shows each region in a few rows and hands back what it laid out, and the
//! keys move what it shows next from there. A run's graph, what it cost, its receipt's page
//! and a workflow checked in the catalog's place each scroll; a run that finished stands as
//! its receipt beside its nodes, whose pane keeps the line it was scrolled to as the person
//! comes back to it, and a successor's starts at its first; a list keeps the row pointed at
//! in sight. A run's pane at work ends with the feed of its node, which alone scrolls there
//! (`feed`).
//!
//! Reached by: `crates/ui/src/stage.rs`, `crates/ui/src/stage/run.rs`,
//! `crates/ui/src/stage/run/feed.rs`, `crates/ui/src/stage/evidence.rs`,
//! `crates/ui/src/stage/stats.rs`, `crates/ui/src/stage/graph.rs`,
//! `crates/ui/src/stage/field.rs`, `crates/surco/src/surface/terminal/scrolled.rs`,
//! `crates/surco/src/surface/terminal/pane.rs`

mod support;

#[path = "scroll/feed.rs"]
mod feed;

use std::path::Path;
use std::sync::Arc;

use surco::components::{Region, Screen, ScreenLaid, Sight};
use surco::glyphs::Glyphs;
use surco::line::Line;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_api::{FollowNews, RunNews};
use yunta_core::events::Channel;
use yunta_core::RunId;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::workspace;
use yunta_testkit_ui::screens::run;
use yunta_ui::prompt::Stroke;
use yunta_ui::{EvidenceOperation, Input, Operation, Performer, Request, State};

use support::{on_the_field, performed, read_whole};

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

/// What the terminal shows of `region` in a pane of [`ROWS`] rows, each row's text.
fn shown(region: &Region) -> Vec<String> {
    let rows = Terminal::on(look()).pane(region, ROWS).lines;
    rows.iter().map(|row| row.text()).collect()
}

/// `state`'s screen shown on the terminal, each region in a pane of [`ROWS`] rows, and what
/// was laid out of each handed back as the terminal hands it back.
fn laid_out(state: &mut State) -> Screen {
    let screen = state.screen(&look());
    let terminal = Terminal::on(look());
    let main = terminal.pane(&screen.main, ROWS).laid;
    let side = screen
        .side
        .as_ref()
        .map(|side| terminal.pane(&side.region, ROWS).laid);
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

/// The region a stage scrolls by the page: the main one, where a run's receipt stands too,
/// its nodes beside it.
fn scrolled(screen: &Screen) -> &Region {
    &screen.main
}

/// Whether the legend offers the keys that scroll a page.
fn offers_paging(screen: &Screen) -> bool {
    screen
        .legend
        .0
        .iter()
        .any(|binding| binding.does == "by the page")
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
/// run, which the field lists with its closed runs once `d` shows them.
async fn at(bench: &Bench, strokes: &[Stroke]) -> State {
    let performer = Arc::new(Performer::new(
        workspace(&bench.worktree, root_of(bench)),
        Channel::Tty,
    ));
    let (field, _) = on_the_field(&[&bench.run_id]).await;
    let strokes: Vec<Stroke> = [Stroke::Insert('d')]
        .into_iter()
        .chain(strokes.iter().copied())
        .collect();
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    state
}

/// A stage a page is shown on: what it is, the strokes that reach it from the field, and
/// whether its legend names the keys that scroll a page at a time once the page scrolls. A
/// run's screen keeps its keys' row for the run's own, as it does over a feed, and so do the
/// pages that stand in the run's frame, its numbers and its graph.
type PagedStage = (&'static str, Vec<Stroke>, bool);

#[tokio::test]
async fn every_page_a_stage_shows_scrolls_by_what_a_surface_laid_out_of_it() {
    let bench = finished("01M7SCROLL0000000000000000").await;
    // What a run cost, as a screen tells it, is a page taller than its rows where the run
    // spent and found something: the design's run at its last gate.
    let spent = run::opened("stats").await;
    scrolls_by_what_was_laid_out(spent.state().clone(), "what it cost", false);
    let stages: [PagedStage; 3] = [
        ("its graph", vec![Stroke::Enter, Stroke::Insert('g')], false),
        (
            "its receipt",
            vec![Stroke::Enter, Stroke::Insert('e')],
            true,
        ),
        (
            "a workflow checked",
            vec![Stroke::Insert('w'), Stroke::Insert('c')],
            true,
        ),
    ];
    for (what, strokes, names_the_keys) in stages {
        let state = at(&bench, &strokes).await;
        scrolls_by_what_was_laid_out(state, what, names_the_keys);
    }
}

/// `state`'s page, `what`, a page taller than its rows, scrolled by the keys from what the
/// terminal laid out of it: from its first line, a page down, its last lines and never past
/// them, its first line again; its legend naming the keys once it scrolls where
/// `names_the_keys`, and never before a surface laid it out.
fn scrolls_by_what_was_laid_out(mut state: State, what: &str, names_the_keys: bool) {
    let before = state.screen(&look());
    assert!(!offers_paging(&before), "{what}: nothing laid out yet");
    let whole = Terminal::on(look()).lines(&scrolled(&before).doc);
    assert!(
        whole.len() > ROWS + A_PAGE,
        "{what} is a page taller than its rows: {:#?}",
        whole.iter().map(Line::text).collect::<Vec<_>>()
    );
    let screen = laid_out(&mut state);
    assert_eq!(
        offers_paging(&screen),
        names_the_keys,
        "{what}: the legend names the keys where the stage's does"
    );
    assert_eq!(scrolled(&screen).sight.top, 0, "{what}");

    let screen = pressed(&mut state, Stroke::PageDown);
    assert_eq!(scrolled(&screen).sight.top, A_PAGE, "{what}: a page down");
    let first = shown(scrolled(&screen)).remove(0);
    assert_eq!(first, whole[A_PAGE].text(), "{what}: shown from there");

    let most = whole.len() - ROWS;
    let screen = pressed(&mut state, Stroke::End);
    let first = shown(scrolled(&screen)).remove(0);
    assert_eq!(first, whole[most].text(), "{what}: the last lines");
    let screen = pressed(&mut state, Stroke::PageDown);
    let first = shown(scrolled(&screen)).remove(0);
    assert_eq!(first, whole[most].text(), "{what}: never past them");
    let screen = pressed(&mut state, Stroke::Home);
    assert_eq!(scrolled(&screen).sight.top, 0, "{what}: the first line");
}

#[tokio::test]
async fn a_receipt_come_back_to_keeps_its_line_and_a_node_pointed_at_opens_on_its_feeds_end() {
    let bench = finished("01M7SCROLLAGAIN00000000000").await;
    let mut state = at(&bench, &[Stroke::Enter]).await;
    laid_out(&mut state);
    let screen = pressed(&mut state, Stroke::PageDown);
    let scrolled_to = scrolled(&screen).sight.top;
    assert!(scrolled_to > 0, "the receipt a page down: {scrolled_to}");
    // What it cost, then back: the receipt stands where it was.
    let performer = Arc::new(Performer::new(
        workspace(&bench.worktree, root_of(&bench)),
        Channel::Tty,
    ));
    let strokes = [Stroke::Insert('s'), Stroke::Decline];
    let (mut state, _) = drive(state, Vec::new(), &performer, &strokes).await;
    let screen = laid_out(&mut state);
    assert_eq!(scrolled(&screen).sight.top, scrolled_to, "kept");
    // The node the pointer moves to from the last that ran shows its own scene in its
    // place, which ends with the node's feed: that alone scrolls there, from its end.
    let (mut state, _) = drive(state, Vec::new(), &performer, &[Stroke::Insert('k')]).await;
    let screen = laid_out(&mut state);
    assert_eq!(screen.trail.last().map(String::as_str), Some("first"));
    assert_eq!(
        scrolled(&screen).sight,
        Sight::at_the_end(),
        "the node's feed, at its end"
    );
}

#[tokio::test]
async fn a_successors_receipt_starts_at_its_first_line() {
    let promoted = finished("01M7SCROLLPROMOTED00000000").await;
    let successor = finished("01M7SCROLLSUCCESSOR0000000").await;
    let (field, _) = on_the_field(&[&promoted.run_id]).await;
    let mut state = field;
    // The run opened, read whole as what the state follows.
    state.update(Input::Stroke(Stroke::Enter));
    let asked = state.update(Input::FollowNews(read_whole(&promoted).await));
    verified(&mut state, &asked, &promoted).await;
    laid_out(&mut state);
    let screen = pressed(&mut state, Stroke::PageDown);
    assert!(scrolled(&screen).sight.top > 0, "the receipt a page down");
    // The work goes on in the successor, whose receipt stands from its first line.
    let FollowNews::Whole { run, events } = read_whole(&successor).await else {
        unreachable!("a run read whole");
    };
    let handed_on = FollowNews::Run {
        run: promoted.run_id.clone(),
        news: RunNews::Successor { run, events },
    };
    let asked = state.update(Input::FollowNews(handed_on));
    verified(&mut state, &asked, &successor).await;
    let screen = laid_out(&mut state);
    assert_eq!(scrolled(&screen).sight.top, 0, "the successor's receipt");
}

/// The event chain of `bench`'s run, which `asked` asks verified as its receipt first
/// shows, verified as the performer verifies it, and handed back to `state`.
async fn verified(state: &mut State, asked: &[Request], bench: &Bench) {
    let verifying = asked.iter().find_map(|request| match request {
        Request::Perform(
            ticket,
            operation @ Operation::Evidence(EvidenceOperation::Verify { .. }),
        ) => Some((*ticket, operation.clone())),
        _ => None,
    });
    let (ticket, verify) = verifying.expect("the receipt's chain is verified as it first shows");
    let (_, verified) = performed(bench, verify).await;
    state.update(Input::Outcome(ticket, verified));
}

#[tokio::test]
async fn a_field_taller_than_its_region_keeps_the_run_pointed_at_in_sight() {
    let runs: Vec<RunId> = (0..12)
        .map(|at| format!("01M7FIELD{at:017}").parse().expect("a run's id"))
        .collect();
    let listed: Vec<&RunId> = runs.iter().collect();
    let (mut state, _) = on_the_field(&listed).await;
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
