//! `a` on a page about no one run, the machine and upkeep, through `State` alone in the place
//! their targets open in, a repository with a run parked on its gate: it goes where the
//! page's attention row names, that run's screen in place of the page, what stood under the
//! page standing under it.

use std::mem::{discriminant, Discriminant};

use surco::look::Look;
use yunta_testkit_ui::screens::home;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Stage, State};

/// The run parked on its gate in the place, which waited longest on the person.
const PARKED: &str = "01M7WALKPARKED000000000000";

/// `a` on the page of the target `name`: the parked run's screen in its place, followed.
async fn goes_to_the_run_its_row_names(name: &str) {
    let page = home::opened(name).await;
    let mut went = page.state().clone();
    let row = page.state().screen(&Look::plain()).attention;
    assert!(row.is_some(), "`{name}`: its attention row names the run");
    went.update(Input::Stroke(Stroke::Insert('a')));
    let followed = went.followed().run.expect("a run's screen follows its run");
    assert_eq!(
        followed.to_string(),
        PARKED,
        "`{name}`: the run the row names"
    );
    let trail = went.screen(&Look::plain()).trail;
    assert_eq!(trail[1], followed.handle(), "`{name}`: its screen");
    assert!(
        matches!(went.stage(), Some(Stage::Run(_))),
        "`{name}`: the run's stage stands where the person is"
    );
    let kinds = |state: &State| -> Vec<Discriminant<Stage>> {
        state.stages_under().map(discriminant).collect()
    };
    assert_eq!(
        kinds(&went),
        kinds(page.state()),
        "`{name}`: in the page's place, what stood under it standing under the run"
    );
}

/// On a deep stack: the place runs whole runs inside the test, which a debug build's futures
/// make deeper than a test thread's stack.
#[test]
fn a_on_the_machine_goes_to_the_run_its_attention_row_names() {
    yunta_testkit::on_a_deep_stack(|| goes_to_the_run_its_row_names("machine"));
}

/// On a deep stack, as the machine's.
#[test]
fn a_on_upkeep_goes_to_the_run_its_attention_row_names() {
    yunta_testkit::on_a_deep_stack(|| goes_to_the_run_its_row_names("upkeep"));
}
