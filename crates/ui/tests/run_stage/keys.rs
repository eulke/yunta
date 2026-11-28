//! What each key does on a run's screen, through `State` alone, on the cuts of the targets
//! of a run's screen: `j k` along the nodes, the node pointed at shown; the arrows (a
//! wheel turn arrives as them), the page keys, `Home`, `End` and the space bar through the
//! pane, the pointer left where it stands; `enter`, `p` and `w` doing nothing; `a` to what
//! the attention row names, the run's own decision in place or another run's screen in its
//! place;
//! `n` a new run where no prompt stands; `s` and `g` its numbers and its graph; `r` and `x`
//! where the scene offers them; and the legend naming the scene's keys, leaving `c`, `e`,
//! `v` and `o` to the palette. `f` giving the lead back is held where the lead is
//! (`tests/lead.rs`).

use surco::components::{Keys, Laid, Screen, ScreenLaid, Sight};
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::{Markdown, Terminal};
use yunta_core::step::NextStep;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Request, Scene, State};

/// The rows the pane is laid out in: a node's pane at the least terminal, where what stands
/// above the feed leaves its window the least it keeps, fewer rows than the feed of a node
/// at work on these cuts holds, so every key that scrolls has rows to move through.
const ROWS: usize = 5;

/// The look a screen is laid out in: Unicode, unpainted, 80 cells.
fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(80),
        motion: Motion::Still,
    }
}

/// `state`'s screen with its pane laid out in [`ROWS`] rows, what was laid out handed
/// back as a terminal hands it back; and what was laid out.
fn laid_out(state: &mut State) -> (Screen, Laid) {
    let screen = state.screen(&look());
    let main = Terminal::on(look()).pane(&screen.main, ROWS).laid;
    let laid = ScreenLaid {
        main,
        side: None,
        overlay: None,
    };
    state.update(Input::Laid(laid));
    (state.screen(&look()), main)
}

/// `stroke` handed to `state`, its pane laid out again: what of the pane it asks to see,
/// and the node the trail names.
fn scrolled_by(state: &mut State, stroke: Stroke) -> (Sight, Option<String>) {
    let asked = state.update(Input::Stroke(stroke));
    assert!(asked.is_empty(), "{stroke:?} asks nothing: {asked:?}");
    let (screen, _) = laid_out(state);
    (screen.main.sight, node_named(&screen))
}

/// The node `screen`'s trail names after the run's handle, where it names one.
fn node_named(screen: &Screen) -> Option<String> {
    match screen.trail.as_slice() {
        [_, _, node] => Some(node.clone()),
        _ => None,
    }
}

/// The last place on `state`'s trail.
fn stands_at(state: &State) -> String {
    let trail = state.screen(&Look::plain()).trail;
    trail.last().cloned().unwrap_or_default()
}

/// `state`'s screen, as Markdown draws it whole.
fn drawn(state: &State) -> String {
    Markdown.screen(&state.screen(&Markdown.look()))
}

/// The keys `state`'s legend names, in its order.
fn legend_keys(state: &State) -> Vec<String> {
    let legend = state.screen(&Look::plain()).legend;
    legend
        .0
        .iter()
        .map(|binding| match &binding.keys {
            Keys::Typed(keys) => keys.clone(),
            Keys::UpDown => "↑↓".to_string(),
            Keys::LeftRight => "←→".to_string(),
            Keys::Typing => "type".to_string(),
        })
        .collect()
}

/// What `state`'s legend says `key` does, where it names it.
fn legend_says(state: &State, key: &str) -> Option<String> {
    let legend = state.screen(&Look::plain()).legend;
    let named = |keys: &Keys| matches!(keys, Keys::Typed(typed) if typed == key);
    let binding = legend.0.into_iter().find(|binding| named(&binding.keys));
    binding.map(|binding| binding.does)
}

/// The commands that do the same as the rows of the palette opened over `state`, in order.
pub(super) fn palette_offers(state: &State) -> Vec<String> {
    let mut opened = state.clone();
    opened.update(Input::Stroke(Stroke::Insert('/')));
    let overlay = opened
        .screen(&Markdown.look())
        .overlay
        .expect("the palette stands over the run");
    let rows = overlay.doc.blocks().iter().find_map(|block| match block {
        surco::doc::Block::ChoiceList(list) => list.choices.as_ref(),
        _ => None,
    });
    let rows = rows.map(|choices| choices.rows.iter());
    rows.into_iter()
        .flatten()
        .filter_map(|row| row.said.clone())
        .collect()
}

/// `key` pressed on a copy of `screen`'s state, which stands as the key leaves it, and what
/// the key asked.
fn pressed_on(screen: &RunScreen, key: Stroke) -> (State, Vec<Request>) {
    let mut state = screen.state().clone();
    let asked = state.update(Input::Stroke(key));
    (state, asked)
}

/// `key` pressed on `screen`, which asks nothing and leaves its screen as it was.
fn does_nothing(screen: &RunScreen, key: Stroke) {
    let (pressed, asked) = pressed_on(screen, key);
    assert!(asked.is_empty(), "{key:?} asks nothing: {asked:?}");
    assert_eq!(
        drawn(&pressed),
        drawn(screen.state()),
        "{key:?} leaves the screen as it was"
    );
}

#[tokio::test]
async fn the_arrows_and_the_page_keys_scroll_the_pane_and_leave_the_pointer_on_its_node() {
    // A loop's task at work: its pane ends with the node's feed, which alone scrolls.
    let building = run::opened("board-moved").await;
    let mut state = building.state().clone();
    let (screen, laid) = laid_out(&mut state);
    assert_eq!(
        screen.main.sight,
        Sight::at_the_end(),
        "the feed at its end"
    );
    let pointed = node_named(&screen);
    assert_eq!(pointed.as_deref(), Some("implement"), "the node at work");

    // A page key moves by the rows the feed's window shows between its two counts.
    let a_page = laid.page - 1;
    let most = laid.most;
    assert!(
        most > 2 + a_page,
        "the feed is longer than a page above its end: {laid:?}"
    );
    let row = Sight::from_line;
    let keys = [
        (Stroke::Up, row(most - 1)),
        (Stroke::Up, row(most - 2)),
        (Stroke::Down, row(most - 1)),
        (Stroke::PageUp, row(most - 1 - a_page)),
        (Stroke::Insert(' '), row(most - 1)),
        (Stroke::Home, row(0)),
        (Stroke::PageDown, row(a_page)),
        (Stroke::End, Sight::at_the_end()),
    ];
    for (key, sight) in keys {
        let moved = scrolled_by(&mut state, key);
        assert_eq!(moved, (sight, pointed.clone()), "{key:?}");
    }
}

#[tokio::test]
async fn j_and_k_move_along_the_nodes_and_the_node_pointed_at_shows() {
    let work = run::opened("work").await;
    let (on_the_next, asked) = pressed_on(&work, Stroke::Insert('j'));
    assert_eq!(
        stands_at(&on_the_next),
        "spec",
        "the node after the one at work"
    );
    assert!(
        asked.is_empty(),
        "the node's scene stands from what is followed: {asked:?}"
    );
    let mut state = on_the_next;
    state.update(Input::Stroke(Stroke::Insert('k')));
    assert_eq!(stands_at(&state), "plan", "back to the node before it");
}

#[tokio::test]
async fn enter_p_and_w_do_nothing_on_a_run() {
    let work = run::opened("work").await;
    does_nothing(&work, Stroke::Enter);
    does_nothing(&work, Stroke::Insert('p'));
    // A run parked on its plan: the desk is the gate's own scene, where `p` is its proof's,
    // and `enter` confirms only a number named.
    let desk = run::opened("desk").await;
    does_nothing(&desk, Stroke::Enter);
    // A run parked on its child: the run's screen is its status.
    let child = run::opened("child").await;
    does_nothing(&child, Stroke::Insert('w'));
}

#[tokio::test]
async fn a_goes_to_the_run_the_attention_row_names_in_place_of_this_one() {
    // The run leads at work: the attention row names the run beside it that waited
    // longest on the person.
    let work = run::opened("work").await;
    let (went, _) = pressed_on(&work, Stroke::Insert('a'));
    let followed = went.followed().run.expect("a run's screen follows its run");
    assert_eq!(
        followed.handle(),
        "EZRPE9",
        "the run the attention row names"
    );
    let trail = went.screen(&Look::plain()).trail;
    assert_eq!(
        trail,
        ["yunta", "EZRPE9"],
        "its screen in place of this one"
    );
    let mut back = went.clone();
    back.update(Input::Stroke(Stroke::Decline));
    let repository = back
        .followed_runs()
        .repository
        .as_ref()
        .and_then(yunta_api::Repository::dir);
    let field = repository.map(|place| place.display().to_string());
    assert_eq!(
        back.screen(&Look::plain()).trail.get(1),
        field.as_ref(),
        "the stage under it is the field this run's stood over"
    );
}

#[tokio::test]
async fn a_on_the_runs_own_decision_goes_to_its_prompt() {
    // The run leads on its failure: its card stands in sight, and stays there.
    let failure = run::opened("failure").await;
    let (went, asked) = pressed_on(&failure, Stroke::Insert('a'));
    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(stands_at(&went), stands_at(failure.state()));
    assert!(matches!(went.scene(), Some(Scene::Card(..))));
    // The card put aside, the person points at the node that waits: the attention row names
    // the run's own decision, and `a` gives the lead back to the run, its card in place.
    let scope = run::opened("node-needs-you").await;
    assert_eq!(
        legend_says(scope.state(), "f").as_deref(),
        Some("follow it")
    );
    let (went, _) = pressed_on(&scope, Stroke::Insert('a'));
    assert!(
        matches!(went.scene(), Some(Scene::Card(..))),
        "{:?}",
        went.scene()
    );
    assert_eq!(
        legend_says(&went, "f"),
        None,
        "the run leads again under its prompt"
    );
}

#[tokio::test]
async fn n_starts_a_new_run_where_no_prompt_stands() {
    let mut work = run::opened("work").await;
    work.pressed(&[Stroke::Insert('n')]).await;
    assert_eq!(stands_at(work.state()), yunta_render::screen::NEW_RUN);
    // A card stands in place: `n` adds a note to its answer, and no new run starts from it.
    let failure = run::opened("failure").await;
    let (noting, asked) = pressed_on(&failure, Stroke::Insert('n'));
    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(stands_at(&noting), stands_at(failure.state()));
    assert_eq!(
        legend_says(&noting, "enter").as_deref(),
        Some("record it"),
        "the card's note is typed"
    );
}

#[tokio::test]
async fn s_and_g_open_its_numbers_and_its_graph_over_it() {
    use yunta_render::screen::{place_of, GRAPH, STATS};
    let work = run::opened("work").await;
    let in_view = work.state().followed_runs().in_view.as_deref();
    let run = &in_view.expect("the run in view").run;
    let (stats, _) = pressed_on(&work, Stroke::Insert('s'));
    assert_eq!(stands_at(&stats), place_of(run.run().handle(), STATS));
    let (graph, _) = pressed_on(&work, Stroke::Insert('g'));
    let workflow = run.manifest().workflow.name.to_string();
    assert_eq!(stands_at(&graph), place_of(&workflow, GRAPH));
}

#[tokio::test]
async fn r_and_x_act_where_the_scene_offers_them() {
    let stopped = run::opened("stopped").await;
    let (_, asked) = pressed_on(&stopped, Stroke::Insert('r'));
    let resumes =
        |request: &Request| matches!(request, Request::Perform(_, Operation::Resume { .. }));
    assert!(asked.iter().any(resumes), "`r` resumes it: {asked:?}");
    let (closing, asked) = pressed_on(&stopped, Stroke::Insert('x'));
    assert!(asked.is_empty(), "{asked:?}");
    let overlay = closing.screen(&Look::plain()).overlay;
    let confirming = overlay.is_some_and(|overlay| overlay.confirms);
    assert!(confirming, "`x` closes it once confirmed");
    // A run at work offers neither.
    let work = run::opened("work").await;
    does_nothing(&work, Stroke::Insert('r'));
    does_nothing(&work, Stroke::Insert('x'));
}

/// A node's own scene, which asks nothing of the run, takes neither `r` nor `x`: the palette
/// lists going on and closing with no key, and each row takes its act, going on at once and
/// closing once confirmed.
#[tokio::test]
async fn on_a_nodes_own_scene_going_on_and_closing_are_the_palettes() {
    // The round put aside, its note taken away by a trip along the nodes and back.
    let mut aside = run::opened("round-aside").await;
    aside
        .pressed(&[Stroke::Insert('j'), Stroke::Insert('k')])
        .await;
    assert!(matches!(aside.state().scene(), Some(Scene::Node)));
    does_nothing(&aside, Stroke::Insert('r'));
    does_nothing(&aside, Stroke::Insert('x'));
    let run = aside.state().followed().run.expect("the run followed");
    let offered = palette_offers(aside.state());
    let row_of = |step: NextStep| {
        let line = yunta_render::command::line(&step);
        let at = offered.iter().position(|row| *row == line);
        at.unwrap_or_else(|| panic!("the palette offers {line}: {offered:?}"))
    };
    let taken = |at: usize| {
        let mut chosen = aside.state().clone();
        chosen.update(Input::Stroke(Stroke::Insert('/')));
        for _ in 0..at {
            chosen.update(Input::Stroke(Stroke::Down));
        }
        let asked = chosen.update(Input::Stroke(Stroke::Enter));
        (chosen, asked)
    };
    let (_, asked) = taken(row_of(NextStep::Resume { run: run.clone() }));
    let resumes =
        |request: &Request| matches!(request, Request::Perform(_, Operation::Resume { .. }));
    assert!(
        asked.iter().any(resumes),
        "its row goes on with it: {asked:?}"
    );
    let (closing, asked) = taken(row_of(NextStep::Close { run }));
    assert!(asked.is_empty(), "{asked:?}");
    let overlay = closing.screen(&Look::plain()).overlay;
    let confirming = overlay.is_some_and(|overlay| overlay.confirms);
    assert!(confirming, "its row closes it once confirmed");
}

/// The rows of the palette opened over `state`, each the command that does the same and the
/// key the run's screen takes it by, where one does.
fn palette_rows(state: &State) -> Vec<(String, Option<String>)> {
    let mut opened = state.clone();
    opened.update(Input::Stroke(Stroke::Insert('/')));
    let overlay = opened.screen(&Markdown.look()).overlay;
    let overlay = overlay.expect("the palette stands over the run");
    let rows = overlay.doc.blocks().iter().find_map(|block| match block {
        surco::doc::Block::ChoiceList(list) => list.choices.as_ref(),
        _ => None,
    });
    let rows = rows.map(|choices| choices.rows.iter());
    rows.into_iter()
        .flatten()
        .filter_map(|row| Some((row.said.clone()?, row.key.clone())))
        .collect()
}

/// A target's cut for each scene a run's screen stands as, outside a dialog and a typed line:
/// work, a board, findings, work held by an answer awaiting its resume, a node's own scene on a
/// card and on a round put aside, a pull request's, each standstill, a run asked elsewhere, a
/// receipt, an end, a card on what it shows and on a scope, and the plan's desk.
const SCENES: [&str; 18] = [
    "work",
    "board",
    "findings",
    "still-answered",
    "node-needs-you",
    "round-aside",
    "pull-request",
    "standstill",
    "no-room",
    "orphans",
    "stopped",
    "stalled",
    "asked-elsewhere",
    "receipt",
    "failed",
    "card",
    "scope",
    "desk",
];

/// On each scene, `r` and `x` take going on and closing exactly where the legend names them:
/// the palette lists the resume and the close each with its key where the legend names that
/// key, with none where it does not. A prompt in place stands with its own legend, which names
/// neither.
#[tokio::test]
async fn r_and_x_take_going_on_and_closing_exactly_where_the_legend_names_them() {
    let mut held: Vec<(&str, &str, bool)> = Vec::new();
    for target in SCENES {
        let screen = run::opened(target).await;
        let state = screen.state();
        let scene = state.scene().expect("a run's scene");
        assert!(!state.typing(), "`{target}`: no line is typed");
        let in_place = matches!(scene, Scene::Round | Scene::Desk | Scene::Card(..));
        let run = state
            .followed()
            .run
            .expect("a run's screen follows its run");
        let rows = palette_rows(state);
        let acts = [
            ("r", NextStep::Resume { run: run.clone() }),
            ("x", NextStep::Close { run }),
        ];
        for (key, step) in acts {
            let line = yunta_render::command::line(&step);
            let Some((_, keyed)) = rows.iter().find(|(said, _)| *said == line) else {
                continue;
            };
            let named = !in_place && legend_says(state, key).is_some();
            assert_eq!(
                keyed.as_deref() == Some(key),
                named,
                "`{target}`: `{key}` takes `{line}` exactly where the legend names it"
            );
            held.push((target, key, named));
        }
    }
    for (key, named) in [("r", true), ("r", false), ("x", true), ("x", false)] {
        assert!(
            held.iter()
                .any(|(_, on, said)| *on == key && *said == named),
            "no scene held `{key}` {}named: {held:?}",
            if named { "" } else { "un" }
        );
    }
}

#[tokio::test]
async fn the_legend_names_the_scenes_keys_and_leaves_c_e_v_o_to_the_palette() {
    let work = run::opened("work").await;
    assert_eq!(legend_keys(work.state()), ["j k", "s", "g", "q"]);
    let run = work.state().followed().run.expect("the run followed");
    let offered = palette_offers(work.state());
    for step in [
        NextStep::Cancel { run: run.clone() },
        NextStep::Open { run: run.clone() },
    ] {
        let line = yunta_render::command::line(&step);
        assert!(
            offered.contains(&line),
            "the palette offers {line}: {offered:?}"
        );
    }
    // `c` stops it all the same, once confirmed.
    let (stopping, _) = pressed_on(&work, Stroke::Insert('c'));
    assert!(
        stopping.screen(&Look::plain()).overlay.is_some(),
        "stopping asks first"
    );

    let stalled = run::opened("stalled").await;
    assert_eq!(
        legend_keys(stalled.state()),
        ["j k", "r", "x", "s", "g", "q"]
    );
    assert_eq!(
        legend_says(stalled.state(), "r").as_deref(),
        Some("continue it from its log")
    );
    assert_eq!(legend_says(stalled.state(), "x").as_deref(), Some("close"));

    let receipt = run::opened("receipt").await;
    assert_eq!(legend_keys(receipt.state()), ["j k", "s", "g", "n", "q"]);
    let run = receipt.state().followed().run.expect("the run followed");
    let offered = palette_offers(receipt.state());
    let receipt_line = yunta_render::command::line(&NextStep::Receipt { run: run.clone() });
    assert!(offered.contains(&receipt_line), "{offered:?}");
    let handle = run.handle();
    let (writing, _) = pressed_on(&receipt, Stroke::Insert('e'));
    assert_eq!(
        stands_at(&writing),
        format!("{handle} {}", yunta_render::screen::RECEIPT),
        "a page over a run takes the run's handle and its word"
    );
}
