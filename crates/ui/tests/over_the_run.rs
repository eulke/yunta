//! `a` on a page that stands in a run's frame, its whole chronicle, its numbers and its graph,
//! through `State` alone on the cuts of their targets: it goes where the page's attention
//! row names, by the rule the run's own screen follows. On the run's own decision, back to
//! the run with the lead given back, its prompt in place as the run's screen brings it; on
//! another run, that run's screen in place of the page.

use surco::components::Keys;
use surco::look::Look;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Scene, State};

/// `key` pressed on `screen`'s state, a copy of it: the state it leaves.
fn pressed_on(screen: &RunScreen, key: Stroke) -> State {
    let mut state = screen.state().clone();
    state.update(Input::Stroke(key));
    state
}

/// The places of `state`'s trail.
fn trail(state: &State) -> Vec<String> {
    state.screen(&Look::plain()).trail
}

/// Whether `state`'s legend names `f`, as a run's legend does while the person holds it.
fn offers_the_lead_back(state: &State) -> bool {
    let legend = state.screen(&Look::plain()).legend;
    let f = |keys: &Keys| matches!(keys, Keys::Typed(typed) if typed == "f");
    legend.0.iter().any(|binding| f(&binding.keys))
}

/// `a` on the run's numbers or its graph, its own decision named in their attention row:
/// back to the run, which leads its screen again, its card in place.
async fn to_its_own_decision(target: &str) {
    let page = run::opened(target).await;
    let went = pressed_on(&page, Stroke::Insert('a'));
    let at = trail(&went);
    assert_eq!(at[1..], ["Q78GRR", "ship"], "`{target}`: back on the run");
    assert!(
        matches!(went.scene(), Some(Scene::Card(..))),
        "`{target}`: its card in place, at {at:?}: {:?}",
        went.scene()
    );
    assert!(!offers_the_lead_back(&went), "`{target}`: the run leads");
}

#[tokio::test]
async fn a_on_the_chronicle_goes_to_the_run_its_attention_row_names() {
    let chronicle = run::opened("chronicle").await;
    let went = pressed_on(&chronicle, Stroke::Insert('a'));
    let followed = went.followed().run.expect("a run's screen follows its run");
    assert_eq!(followed.handle(), "EZRPE9", "the run the row names");
    assert_eq!(
        trail(&went)[..2],
        ["yunta", "EZRPE9"],
        "its screen in place of the chronicle"
    );
}

#[tokio::test]
async fn a_on_the_runs_numbers_goes_to_its_own_decision() {
    to_its_own_decision("stats").await;
}

#[tokio::test]
async fn a_on_the_runs_graph_goes_to_its_own_decision() {
    to_its_own_decision("graph").await;
}
