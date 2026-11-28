//! See [`super`]. A run started from the first request is on its way into the field until
//! the field holds it: a field left for before then lists no run in sight and still keeps the
//! keys that act on a run, so its legend never stands short on the way to a field with runs.

use surco::look::Look;
use yunta_testkit_ui::driving::drive;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, State};

use super::{started_from_the_first_request, STARTED};

/// The keys `state`'s legend names, in its order.
fn legend_keys(state: &State) -> Vec<String> {
    let legend = state.screen(&Look::plain()).legend.0;
    legend
        .iter()
        .map(|binding| binding.keys.named().to_string())
        .collect()
}

/// A run started from the first request, left by `q` before the field the state keeps lists
/// it: the field stands with no run in sight yet, and its legend keeps the keys that act on a
/// run, since the run started is on its way. Once the field holds it, closed and so not listed
/// at a glance, a new run and quitting stand alone.
#[test]
fn a_field_a_run_started_here_is_yet_to_stand_in_keeps_the_keys_that_act_on_a_run() {
    yunta_testkit::on_a_deep_stack(|| async {
        let started = started_from_the_first_request().await;
        let (mut state, performer) = (started.state, started.performer);
        let asked = state.update(Input::Stroke(Stroke::Insert('q')));
        let field = state.followed_runs().field.as_ref();
        assert!(
            field.is_some_and(|field| !field.runs.contains_key(&STARTED)),
            "the field kept does not list the run yet: {field:?}"
        );
        assert_eq!(legend_keys(&state), ["j k", "enter", "n", "x", "q"]);

        let (state, _) = drive(state, asked, &performer, &[]).await;
        let field = state.followed_runs().field.as_ref();
        assert!(field.is_some_and(|field| field.runs.contains_key(&STARTED)));
        assert_eq!(legend_keys(&state), ["n", "q"]);
    });
}
