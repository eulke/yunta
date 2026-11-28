//! A new run's adapter row: it offers each runner's own, then the adapters a run here can be
//! pinned to, those the runners name that this binary builds and no other, so no choice it
//! offers is one admission is known to refuse; a new run opened again keeps its adapter only
//! while the runners still name it.

use std::path::Path;
use std::sync::Arc;

use yunta_core::events::Channel;
use yunta_core::RunAdapter;
use yunta_render::page::NewRun;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Performer, Request, State};

use super::{
    drawn, on_the_first_request_with, pressed, running_on, set_up_with, ticket_of, typed, HELD,
    IN_PLACE, NO_SESSIONS,
};

/// The adapter row of a new run `drawn` in Markdown: from its heading, the last before what
/// runs, to what runs. The panel beside the rows, which Markdown says first, may name an
/// adapter of the machine's.
fn adapter_row(drawn: &str) -> &str {
    let foot = drawn
        .find(NewRun::WHAT_RUNS)
        .expect("what runs, under the rows");
    let heading = drawn[..foot].rfind(NewRun::ADAPTER);
    &drawn[heading.expect("the adapter's heading, over its row")..foot]
}

/// A new run opened from the first request of a repository set up with `config`, its catalog
/// read and what doctor finds not yet, the keys on its adapter row: the state, and its adapter
/// row as drawn.
async fn on_the_adapter_row(root: &Path, config: &str) -> (State, String) {
    let (mut state, performer) = on_the_first_request_with(root, config).await;
    let mut strokes = typed("greet ana");
    strokes.push(Stroke::Enter);
    let opening = pressed(&mut state, &strokes);
    let catalog = ticket_of(&opening, |operation| {
        matches!(operation, Operation::ReadCatalog)
    });
    let catalog = catalog.expect("the new run asks the catalog as it opens");
    let read = performer.workspace().catalog().await;
    state.update(Input::Outcome(catalog, Outcome::Catalog(Box::new(read))));
    // Back from the request, round to the last row.
    pressed(&mut state, &[Stroke::BackTab]);
    let row = adapter_row(&drawn(&state)).to_string();
    (state, row)
}

/// Where the runners name one of the adapters this binary builds, the adapter row offers each
/// runner's own and that one, never another this binary builds, which admission would refuse
/// to pin: `→` there chooses it, and `enter` asks for the run pinned to it.
#[tokio::test]
async fn the_adapter_row_offers_only_the_adapters_the_runners_name() {
    let built = yunta_api::machine::built_adapter_ids();
    let (Some(unnamed), Some(named)) = (built.first(), built.last()) else {
        panic!("this binary builds no adapter");
    };
    assert_ne!(unnamed, named, "this binary builds two adapters: {built:?}");
    let root = tempfile::tempdir().unwrap();
    let (mut state, row) = on_the_adapter_row(root.path(), &running_on(&[named])).await;
    assert!(
        row.contains(NewRun::AS_THE_RUNNERS_SAY) && row.contains(named.as_str()),
        "the row offers each runner's own and the adapter the runners name: {row}"
    );
    assert!(
        !row.contains(unnamed.as_str()),
        "the row offers no adapter the runners do not name: {row}"
    );
    let asked = pressed(&mut state, &[Stroke::Right, Stroke::Enter]);
    let [Request::Perform(_, Operation::Start { request })] = asked.as_slice() else {
        panic!("the run is asked for: {asked:?}");
    };
    assert_eq!(request.adapter, Some(RunAdapter::Named(named.clone())));
}

/// Where the runners name no adapter, the adapter row holds each runner's own alone: `→` there
/// has nowhere to go, and `enter` asks for the run with no adapter pinned.
#[tokio::test]
async fn the_adapter_row_holds_the_runners_own_alone_where_they_name_no_adapter() {
    let root = tempfile::tempdir().unwrap();
    let (mut state, row) = on_the_adapter_row(root.path(), IN_PLACE).await;
    assert!(row.contains(NewRun::AS_THE_RUNNERS_SAY), "{row}");
    for built in yunta_api::machine::built_adapter_ids() {
        assert!(
            !row.contains(built.as_str()),
            "the row offers no adapter the runners do not name: {row}"
        );
    }
    let asked = pressed(&mut state, &[Stroke::Right, Stroke::Enter]);
    let [Request::Perform(_, Operation::Start { request })] = asked.as_slice() else {
        panic!("the run is asked for: {asked:?}");
    };
    assert_eq!(request.adapter, None);
}

/// A new run left unstarted with an adapter chosen, opened again once the runners name it no
/// more, takes the catalog it is opened with: its adapter row holds the runners' own alone,
/// chosen, and `enter` asks for the run with no adapter pinned.
#[tokio::test]
async fn a_new_run_opened_again_keeps_its_adapter_only_while_the_runners_name_it() {
    let named = yunta_api::machine::first_built_adapter();
    let world = Bench::with_run_id(HELD.as_str());
    set_up_with(&world.worktree, &running_on(&[&named]));
    world.run(FINISHES, NO_SESSIONS).await;
    let root = world
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Arc::new(Performer::new(
        workspace(&world.worktree, root),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let chosen = [Stroke::Insert('n'), Stroke::BackTab, Stroke::Right];
    let (state, _) = drive(opened, asked, &performer, &chosen).await;
    let pinned = drawn(&state);
    assert!(
        adapter_row(&pinned).contains(&format!("▸ **{named}**")),
        "{pinned}"
    );
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Decline]).await;

    std::fs::write(world.worktree.join(".yunta/config.yaml"), IN_PLACE).unwrap();
    let again = [Stroke::Insert('n')];
    let (mut state, _) = drive(state, Vec::new(), &performer, &again).await;
    let drawn = drawn(&state);
    let row = adapter_row(&drawn);
    assert!(
        row.contains(&format!("▸ **{}**", NewRun::AS_THE_RUNNERS_SAY)),
        "{row}"
    );
    assert!(!row.contains(named.as_str()), "{row}");
    let asked = pressed(&mut state, &[Stroke::Enter]);
    let Some(Operation::Start { request }) = asked.iter().find_map(|request| match request {
        Request::Perform(_, operation @ Operation::Start { .. }) => Some(operation),
        _ => None,
    }) else {
        panic!("the run is asked for: {asked:?}");
    };
    assert_eq!(request.adapter, None);
}
