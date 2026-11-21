//! A state driven with its performer stands caught up with the log it follows before a
//! stroke is pressed and once it is handed back: a following has no ticket a state waits
//! on, so only the log says what it has yet to hear.

use yunta_api::Awaited;
use yunta_core::step::NextStep;
use yunta_testkit_ui::screens::World;
use yunta_ui::AwaitedReading;

/// The screens' world opened on `review` of its parked run and driven with no stroke
/// stands caught up with its log: the field and the run read at each run's head, and the
/// decision the run waits on read.
#[test]
fn a_state_driven_with_no_stroke_stands_caught_up_with_its_log() {
    yunta_testkit::on_a_deep_stack(|| async {
        let world = World::made().await;
        let review = NextStep::Review {
            run: world.parked().clone(),
        };
        let state = world.driven_from(&review, &[]).await;
        assert!(
            state.caught_up(&world.heads()),
            "the state stands behind the log: {:?}",
            state.followed()
        );
        let in_view = state.followed_runs().in_view.as_ref();
        let awaited = in_view.and_then(|in_view| in_view.awaited.as_ref());
        assert!(
            matches!(
                awaited,
                Some(AwaitedReading::Read {
                    awaited: Awaited::Decision(_),
                    ..
                })
            ),
            "the decision the parked run waits on is read: {awaited:?}"
        );
    });
}
