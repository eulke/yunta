//! A screen that says work is under way waits on that very work: the stage on screen says, as
//! the work it was asked as, what its screen shows under way (`State::shown_under_way`), a
//! reading or an act, and the state waits on it. A page never stands on work nothing answers,
//! whether a stage stood over it while its work went on or what it asked was let go.

use yunta_ui::State;

use crate::walk::Reached;

/// `state`, reached as `reached` says: where its screen shows work under way, the state waits
/// on that work.
pub(super) fn under_way_waited_on(state: &State, reached: &Reached<'_>) {
    let Some(shown) = state.shown_under_way() else {
        return;
    };
    let waits = state
        .waiting_on()
        .iter()
        .any(|(_, waited)| *waited == shown);
    assert!(
        waits,
        "the screen shows {shown:?} under way and the state does not wait on it, reached by:\n{}",
        reached.inputs()
    );
}
