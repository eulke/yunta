//! What a run cost and how a workflow is drawn, in the application: a run's stats as its
//! screen tells them, and its graph, each node a box holding its id, headed by its mode; both
//! moving with the run's news, with no stroke, and onto the successor a promotion hands its
//! work to; both in the run's frame, naming its decision where it waits on the person; and,
//! from the catalog, a workflow's past runs as `yunta stats --workflow` prints them and its
//! graph as written.

mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use surco::components::ScreenLaid;
use surco::doc::Depth;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::surface::{Markdown, Surface, Terminal};
use surco::words::Standing;
use tokio::sync::mpsc;
use yunta_api::{FieldReach, FollowNews, Followed, Stop};
use yunta_core::events::Channel;
use yunta_core::{RunId, Workflow, WorkflowName};
use yunta_render::graph;
use yunta_render::page::{Graph, History, Page, Stats, StatsTold};
use yunta_render::screen::{place_of, GRAPH, STATS};
use yunta_testkit::workflows::PROMOTABLE;
use yunta_testkit::{Bench, ScriptedInteraction};
use yunta_testkit_core::fixed_now;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_testkit_ui::screens::run;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, State};

use support::{on_the_field, performer_in, stopped_beside};

/// Two nodes that finish, one after the other.
const FINISHES: &str = "name: done\nnodes:\n  - { id: first, kind: bash, run: \"true\" }\n  \
                        - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// A node an agent works, then one that finishes after it: what the agent's session
/// ran on is a row only the printed stats show.
const WORKED: &str =
    "name: worked\nnodes:\n  - id: first\n    kind: prompt\n    runner: executor\n    \
                      prompt: \"Say it is done.\"\n  \
                      - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// The agent's one session, which ends done.
const SAYS_DONE: &str =
    "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: done }\n";

/// A repository's configuration that prices a model: what a run cost then says what its
/// tokens cost.
const PRICED: &str = "defaults:\n  isolation: none\npricing:\n  claude-opus:\n    \
                      cost_per_1k_tokens: 0.015\n";

/// A run whose stats stand over its screen while it moves.
const COSTED: RunId = RunId::from_static("01M7C0STEDM0VES00000000000");

/// A run whose graph stands over its screen while it moves.
const DRAWN: RunId = RunId::from_static("01M7DRAWNM0VES000000000000");

/// A run whose stats stand over its screen as it is promoted.
const SPENT_ON: RunId = RunId::from_static("01M7STATSPR0M0TED000000000");

/// A run whose graph stands over its screen as it is promoted.
const DRAWN_ON: RunId = RunId::from_static("01M7GRAPHPR0M0TED000000000");

/// How long a following takes at most to hand a promoted run's successor on: its looks
/// come a tenth of a second apart, so only a successor never handed waits half a minute.
const HANDING_ON: Duration = Duration::from_secs(30);

/// The rows a terminal gives the main region: fewer than the stats or the graph of a run
/// of [`PROMOTABLE`] take, so either scrolls.
const ROWS: usize = 4;

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

/// The application on the field of `world`'s repository, set up with a configuration that
/// prices a model, its one run `run` created on `workflow` in the world's mode and
/// stopped as a person stops it, as the follower reads it: the state, the repository's
/// world, and the run's bench, which wakes it.
async fn on_a_priced_field(world: Bench, run: &RunId, workflow: &str) -> (State, Bench, Bench) {
    let config = world.worktree.join(".yunta/config.yaml");
    let dir = config.parent().expect("the repository's .yunta");
    tokio::fs::create_dir_all(dir).await.expect("its .yunta");
    tokio::fs::write(&config, PRICED).await.expect("its config");
    let stopped = stopped_beside(&world, run, workflow).await;
    let (opened, asked) = State::opened(now());
    let (state, surfaced) = drive(opened, asked, &performer_in(&world), &[]).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    (state, world, stopped)
}

/// `bench`'s run woken with nothing stopping it: it runs its nodes to its end.
async fn woken(bench: Bench) {
    bench.with_cancel(Stop::new().work().clone()).wake().await;
}

/// `bench`'s stopped run of [`PROMOTABLE`] woken with nothing stopping it, its failure
/// answered `promote`: it closes promoted, and the successor in the full mode it asks for
/// is created, not yet woken. The successor's id.
async fn promoted(bench: Bench) -> RunId {
    let bench = bench.with_cancel(Stop::new().work().clone());
    bench
        .wake_answering(&ScriptedInteraction::choose("promote"))
        .await;
    bench.successor("full").await
}

/// The line `state`'s main region shows first once a page down is pressed, as a terminal
/// showing that region in [`ROWS`] rows laid it out.
fn a_page_down(state: &mut State) -> usize {
    let look = Look::plain();
    let screen = state.screen(&look);
    let (_, main) = Terminal::on(look).scrolled(&screen.main.doc, screen.main.sight, ROWS);
    state.update(Input::Laid(ScreenLaid {
        main,
        side: None,
        overlay: None,
    }));
    state.update(Input::Stroke(Stroke::PageDown));
    state.screen(&look).main.sight.top
}

/// The application on a field whose one run `run`, of [`PROMOTABLE`], stopped, has the
/// stage `key` opens over its own scrolled a page down, then is promoted: the state once
/// it keeps the successor in view, at the instant the project's clock reads, the
/// repository's world, and the successor's id.
async fn over_a_promoted_run(run: &RunId, key: char) -> (State, Bench, RunId) {
    let world = Bench::new().in_mode("quick");
    let (state, world, stopped) = on_a_priced_field(world, run, PROMOTABLE).await;
    let performer = performer_in(&world);
    let strokes = [Stroke::Enter, Stroke::Insert(key)];
    let (mut state, surfaced) = drive(state, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    assert!(
        a_page_down(&mut state) > 0,
        "the page is taller than its rows"
    );

    let (mut state, successor) = promoted_while_followed(state, &performer, stopped).await;
    state.update(Input::Time(fixed_now()));
    (state, world, successor)
}

/// `state` handed what `performer` follows for it while `bench`'s stopped run is promoted,
/// until it keeps the successor in view, waits on nothing and stands caught up with the
/// log: the state, and the successor's id. One following stands throughout, made and
/// reading the run whole before the promotion, as a surface keeps one, so the promotion
/// is one it sees come. Not `drive`, which makes a following of its own at each call. The
/// performer surfaces nothing but a call for the person: while the bench's engine asks at
/// its terminal the run's registry says it asks, and a look that lands then calls them.
async fn promoted_while_followed(
    mut state: State,
    performer: &Arc<Performer>,
    bench: Bench,
) -> (State, RunId) {
    let project = performer
        .workspace()
        .project()
        .expect("the performer's project");
    let (inputs, mut received) = mpsc::unbounded_channel();
    let mut asked = vec![Request::Follow(state.followed())];
    let mut stopped = Some(bench);
    let mut successor: Option<RunId> = None;
    loop {
        for request in asked.drain(..) {
            let surfaced = performer.take(request, &inputs);
            let calls = matches!(surfaced, None | Some(Request::CallThePerson));
            assert!(calls, "{surfaced:?}");
        }
        if let Some(successor) = successor.as_ref() {
            let in_view = state.followed_runs().in_view.as_ref();
            let onto = in_view.is_some_and(|in_view| in_view.run.run == *successor);
            let heads = yunta_testkit::heads(&project.storage_path);
            if onto && state.waiting_on().is_empty() && state.caught_up(&heads) {
                return (state, successor.clone());
            }
        }
        let came = tokio::time::timeout(HANDING_ON, received.recv()).await;
        let input = came.expect("the successor is handed on");
        let input = input.expect("the performer hands what it reads");
        let read_whole = matches!(input, Input::FollowNews(FollowNews::Whole { .. }));
        asked = state.update(input);
        if let Some(bench) = stopped.take_if(|_| read_whole) {
            successor = Some(promoted(bench).await);
        }
    }
}

/// What the application follows over a run's stage opened from the field here.
fn following(run: &RunId) -> Followed {
    Followed {
        field: Some(FieldReach::Here),
        run: Some(run.clone()),
    }
}

/// A run's stats over its screen move with the run's news, with no stroke; drawn at one
/// instant, they are what a run's screen tells of the run then, what it cost at the prices
/// the project's configuration names left to the printed page.
#[tokio::test]
async fn a_runs_stats_move_with_its_news_and_are_what_a_screen_tells_of_the_run() {
    let (state, world, stopped) = on_a_priced_field(Bench::new(), &COSTED, FINISHES).await;
    let performer = performer_in(&world);
    let strokes = [Stroke::Enter, Stroke::Insert('s')];
    let (state, surfaced) = drive(state, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let before = main_region(&state);

    woken(stopped).await;
    let (mut state, _) = drive(state, Vec::new(), &performer, &[]).await;
    let after = main_region(&state);
    assert_ne!(after, before, "the stats move with the run's news");

    // The instant the project's clock reads, at which `yunta stats` prints.
    state.update(Input::Time(fixed_now()));
    let project = workspace(&world.worktree, root_of(&world))
        .project()
        .expect("the project");
    let spent = project.run(&COSTED).await.expect("the run").stats();
    let told = |told| {
        drawn(&Stats {
            frame: &spent.frame,
            stats: &spent.stats,
            pricing: spent.pricing.as_ref(),
            told,
        })
    };
    let printed = told(StatsTold::Printed(Depth::Glance));
    assert!(
        printed.contains("avg of 1 priced model"),
        "costed at the project's prices: {printed}"
    );
    let on_screen = told(StatsTold::OnScreen);
    assert!(!on_screen.contains("avg of 1 priced model"), "{on_screen}");
    assert_eq!(main_region(&state), on_screen);
}

/// A run's graph over its screen holds each node by its id alone as the run moves, with
/// no stroke: before its node finishes and after, it is the page of the workflow the run
/// froze, headed by the mode it runs in, and no node's word stands in it.
#[tokio::test]
async fn a_runs_graph_holds_each_node_by_its_id_as_the_run_moves() {
    let (state, world, stopped) = on_a_priced_field(Bench::new(), &DRAWN, FINISHES).await;
    let performer = performer_in(&world);
    let strokes = [Stroke::Enter, Stroke::Insert('g')];
    let (state, surfaced) = drive(state, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let before = main_region(&state);

    woken(stopped).await;
    let (state, _) = drive(state, Vec::new(), &performer, &[]).await;
    let after = main_region(&state);
    let project = workspace(&world.worktree, root_of(&world))
        .project()
        .expect("the project");
    let open = project.run(&DRAWN).await.expect("the run");
    let frame = open.frame();
    assert!(
        frame
            .nodes
            .iter()
            .all(|node| node.standing == Standing::Finished),
        "the run moved on to its end"
    );
    assert_eq!(after, before, "a node's word is no part of the graph");
    assert!(!after.contains(": finished"), "{after}");
    let page = Graph {
        workflow: open.manifest.doc.workflow.clone(),
        mode: Some(frame.mode),
    };
    assert_eq!(after, drawn(&page));
}

/// A run's stats over its screen, the run promoted, move onto its successor with no
/// stroke: its handle, what a screen tells of it, from their first line; and the
/// successor is what the application follows.
#[tokio::test]
async fn a_promoted_runs_stats_move_onto_its_successor() {
    let (state, world, successor) = over_a_promoted_run(&SPENT_ON, 's').await;
    let screen = state.screen(&Markdown.look());
    let place = place_of(successor.handle(), STATS);
    assert_eq!(screen.trail, ["yunta", &place]);
    assert_eq!(screen.main.sight.top, 0, "from their first line");
    let project = workspace(&world.worktree, root_of(&world))
        .project()
        .expect("the project");
    let spent = project
        .run(&successor)
        .await
        .expect("the successor")
        .stats();
    let on_screen = drawn(&Stats {
        frame: &spent.frame,
        stats: &spent.stats,
        pricing: spent.pricing.as_ref(),
        told: StatsTold::OnScreen,
    });
    assert_eq!(main_region(&state), on_screen);
    assert_eq!(state.followed(), following(&successor));
}

/// A run's graph over its screen, the run promoted, moves onto its successor with no
/// stroke: the workflow it froze headed by the mode it was promoted to, from its first
/// line; and the successor is what the application follows.
#[tokio::test]
async fn a_promoted_runs_graph_moves_onto_its_successor() {
    let (state, world, successor) = over_a_promoted_run(&DRAWN_ON, 'g').await;
    let screen = state.screen(&Markdown.look());
    assert_eq!(screen.main.sight.top, 0, "from its first line");
    let project = workspace(&world.worktree, root_of(&world))
        .project()
        .expect("the project");
    let open = project.run(&successor).await.expect("the successor");
    let workflow = open.manifest.doc.workflow.name.to_string();
    assert_eq!(screen.trail, ["yunta", &place_of(&workflow, GRAPH)]);
    let page = Graph {
        workflow: open.manifest.doc.workflow.clone(),
        mode: Some(open.frame().mode),
    };
    assert_eq!(main_region(&state), drawn(&page));
    assert_eq!(state.followed(), following(&successor));
}

/// A finished run's stats are what a run's screen tells of it, which the printed page says
/// more than, and no key unfolds them there.
#[tokio::test]
async fn a_finished_runs_stats_are_what_a_screen_tells_and_the_printed_page_says_more() {
    let bench = finished("01M7STATS00000000000000000", WORKED, SAYS_DONE).await;
    let performer = Arc::new(Performer::new(
        workspace(&bench.worktree, root_of(&bench)),
        Channel::Tty,
    ));
    let (field, _) = on_the_field(&[&bench.run_id]).await;
    // The run opened, then what it cost: the field lists a finished run with its closed
    // runs, which `d` shows.
    let strokes = [Stroke::Insert('d'), Stroke::Enter, Stroke::Insert('s')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let project = workspace(&bench.worktree, root_of(&bench))
        .project()
        .expect("the project");
    let spent = project.run(&bench.run_id).await.expect("the run").stats();
    let told = |told| {
        drawn(&Stats {
            frame: &spent.frame,
            stats: &spent.stats,
            pricing: spent.pricing.as_ref(),
            told,
        })
    };
    let on_screen = told(StatsTold::OnScreen);
    assert_eq!(main_region(&state), on_screen);
    let trail = state.screen(&Markdown.look()).trail;
    let handle = bench.run_id.handle();
    assert_eq!(trail.last(), Some(&place_of(handle, STATS)));
    assert_ne!(
        told(StatsTold::Printed(Depth::Glance)),
        on_screen,
        "the printed page says more"
    );
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Insert('f')]).await;
    assert_eq!(main_region(&state), on_screen, "no key unfolds them");
}

/// A finished run's graph draws the workflow it froze, each node by its id alone, where
/// what `yunta graph --run` prints labels each node with its word.
#[tokio::test]
async fn a_runs_graph_draws_each_node_by_its_id() {
    let bench = finished("01M7GRAPH00000000000000000", FINISHES, NO_SESSIONS).await;
    let performer = Arc::new(Performer::new(
        workspace(&bench.worktree, root_of(&bench)),
        Channel::Tty,
    ));
    let (field, _) = on_the_field(&[&bench.run_id]).await;
    // The field lists a finished run with its closed runs, which `d` shows.
    let strokes = [Stroke::Insert('d'), Stroke::Enter, Stroke::Insert('g')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let project = workspace(&bench.worktree, root_of(&bench))
        .project()
        .expect("the project");
    let open = project.run(&bench.run_id).await.expect("the run");
    let (workflow, frame) = (open.manifest.doc.workflow.clone(), open.frame());
    let shown = main_region(&state);
    let diagram = graph::drawn(&workflow);
    assert!(
        shown.contains(diagram.trim_end()),
        "{shown}\n---\n{diagram}"
    );
    for node in ["[\"first\"]", "[\"second\"]"] {
        assert!(shown.contains(node), "{node} holds its id alone: {shown}");
    }
    assert!(!shown.contains(": finished"), "{shown}");
    let page = Graph {
        workflow,
        mode: Some(frame.mode),
    };
    let named = page.workflow.name.to_string();
    assert_eq!(shown, drawn(&page));
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail.last(), Some(&place_of(&named, GRAPH)));
}

#[tokio::test]
async fn a_workflows_past_runs_and_its_graph_open_from_the_catalog() {
    let bench = finished("01M7HISTORY000000000000000", FINISHES, NO_SESSIONS).await;
    let workflows = bench.worktree.join(".yunta/workflows");
    std::fs::create_dir_all(&workflows).expect("the catalog's directory");
    // Offered by its file's stem, its runs kept under the name the file gives it.
    std::fs::write(workflows.join("finishes.yaml"), FINISHES).expect("the workflow that ran");
    let performer = Arc::new(Performer::new(
        workspace(&bench.worktree, root_of(&bench)),
        Channel::Tty,
    ));
    let (field, _) = on_the_field(&[&bench.run_id]).await;
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
        mode: None,
    };
    assert_eq!(main_region(&state), drawn(&page));
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail, ["yunta", "workflows", "finishes", "graph"]);
}

/// `stats.unicode-120` and `graph.unicode-120` row 35: the design's run at its last gate.
const ITS_DECISION_OUT_OF_SIGHT: &str = "◆ Q78GRR needs you: The work beside its plan, and every finding the run holds with its answer. Create t…  · a go there";

/// A run's numbers and graph name its decision, out of sight under them, and where `a` goes.
#[tokio::test]
async fn stats_over_a_run_waiting_on_the_person_name_its_decision_in_the_attention_row() {
    let mut widest = Look::plain();
    (widest.glyphs, widest.width) = (Glyphs::Unicode, Width::exact(118));
    for target in ["stats", "graph"] {
        let screen = run::opened(target).await;
        let attention = screen.state().screen(&widest).attention;
        let row = attention.map(|it| Terminal::on(widest).attention(&it).text());
        assert_eq!(
            row.as_deref().map(str::trim_end),
            Some(ITS_DECISION_OUT_OF_SIGHT),
            "`{target}`"
        );
    }
}
