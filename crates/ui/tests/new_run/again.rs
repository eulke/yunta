//! See [`super`]. A new run left unstarted, opened again: on the catalog it is opened with,
//! what its rows hold kept by name, and pointing, before that catalog is read, at a row its
//! rows hold.

use std::sync::Arc;

use yunta_core::events::Channel;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Operation, Performer, Request, State};

use super::{drawn, pressed, set_up_in, typed, HELD, NO_SESSIONS};

/// A new run left on its workflow's row and opened again points, before its catalog is read
/// again, at a row its rows hold: the request, since the workflow's row stands only once the
/// catalog is read, so what is typed meanwhile goes into the request.
#[tokio::test]
async fn a_new_run_opened_again_points_at_a_row_it_holds_before_its_catalog_is_read() {
    let world = Bench::with_run_id(HELD.as_str());
    set_up_in(&world.worktree);
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
    let left = [Stroke::Insert('n'), Stroke::Tab, Stroke::Decline];
    let (mut state, _) = drive(opened, asked, &performer, &left).await;
    let asked = pressed(&mut state, &[Stroke::Insert('n')]);
    assert!(
        asked
            .iter()
            .any(|request| matches!(request, Request::Perform(_, Operation::ReadCatalog))),
        "opened again, it reads its catalog: {asked:?}"
    );
    pressed(&mut state, &typed("ana"));
    assert!(drawn(&state).contains("> ana"), "{}", drawn(&state));
}

/// A workflow beside `greet` that reads whom it greets by the same name.
const HELLOS: &str = "name: hello\ninputs:\n  ask: { type: string, request: true }\n  \
                      who: { type: string, description: \"whom it says hello to\" }\nnodes:\n  \
                      - { id: say, kind: bash, run: \"true\" }\n";

/// A workflow the repository gains while a new run stands left unstarted.
const LATER: &str = "name: later\nnodes:\n  - { id: say, kind: bash, run: \"true\" }\n";

/// A new run left unstarted with `hello` chosen and whom to greet typed is opened again, once
/// `hello` is gone and `later` came, on the catalog read as it opens again: `later` listed,
/// the first workflow chosen in `hello`'s place, as moving to another workflow chooses, and
/// whom to greet kept by its name.
#[tokio::test]
async fn a_new_run_opened_again_takes_the_catalog_it_is_opened_with() {
    let world = Bench::with_run_id(HELD.as_str());
    set_up_in(&world.worktree);
    let workflows = world.worktree.join(".yunta/workflows");
    std::fs::write(workflows.join("hello.yaml"), HELLOS).unwrap();
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
    let mut left = vec![Stroke::Insert('n'), Stroke::Tab, Stroke::Down, Stroke::Tab];
    left.extend(typed("bob"));
    left.push(Stroke::Decline);
    let (state, _) = drive(opened, asked, &performer, &left).await;

    std::fs::remove_file(workflows.join("hello.yaml")).unwrap();
    std::fs::write(workflows.join("later.yaml"), LATER).unwrap();
    let again = [Stroke::Insert('n'), Stroke::Tab, Stroke::Tab];
    let (state, _) = drive(state, Vec::new(), &performer, &again).await;
    let drawn = drawn(&state);
    assert!(drawn.contains("later"), "the catalog read again: {drawn}");
    assert!(!drawn.contains("hello"), "{drawn}");
    assert!(drawn.contains("▸ **greet**"), "{drawn}");
    assert!(
        drawn.contains("bob"),
        "whom to greet, kept by its name: {drawn}"
    );
}
