//! A run never fills the disk it works on: one about to go under the part
//! of it the project's pool keeps free stops, and says how to make room.

use yunta_core::events::PauseReason;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

const ONE_STEP: &str = "name: one-step\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n";

#[tokio::test]
async fn a_run_that_would_fill_the_disk_pauses_saying_how_to_make_room() {
    let bench = Bench::new();
    let config = format!("{MOCK_CONFIG}storage:\n  pool:\n    keep_free: \"100%\"\n");

    let RunReport { terminal, state } = bench
        .run_with_config(ONE_STEP, "sessions: []\n", &config)
        .await;

    let RunTerminal::Paused { reason } = terminal else {
        panic!("the run stops before filling the disk: {terminal:?}");
    };
    assert!(matches!(reason, PauseReason::NoRoom { .. }), "{reason:?}");
    let words = yunta_render::pause::sentence(&reason);
    assert!(
        words.starts_with("no room: the disk has") && words.contains("`yunta gc`"),
        "{words}"
    );
    assert!(state.nodes.state("only").is_none(), "nothing ran");
}
