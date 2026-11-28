//! The field's legend names the keys that serve where it stands: where it is read and lists no
//! run in sight, a new run and quitting alone, since moving, opening and closing have no run to
//! act on; where it lists one, the design's five keys; and before it is read, which no person
//! takes for a field with no run, the five keys too.

use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Performer, Stage, State};

use super::support::{on_the_field, performer_in};
use super::{pressed, FIRST};

/// A mock fixture with no sessions: the finished run opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The design's five keys of the field, in a repository where a run can be asked for.
const FIVE_KEYS: &str = "j k move · enter open · n new run · x close · q quit";

/// The keys of the field read with no run in sight, there.
const NEW_RUN_AND_QUIT: &str = "n new run · q quit";

/// What the legend of `state`'s screen says, each key beside what it does.
fn legend(state: &State) -> String {
    let legend = state.screen(&surco::surface::Markdown.look()).legend;
    let said: Vec<String> = legend
        .0
        .iter()
        .map(|binding| format!("{} {}", binding.keys.named(), binding.does))
        .collect();
    said.join(" · ")
}

/// The field `esc` goes to from the first request of a repository with no run, read: a new
/// run and quitting stand in its legend, and no key that acts on a run.
#[tokio::test]
async fn a_field_read_with_no_run_names_a_new_run_and_quitting_alone() {
    let (state, world) = on_the_field(&[]).await;
    assert!(matches!(state.stage(), Some(Stage::FirstRun(_))));
    let (state, _) = drive(state, Vec::new(), &performer_in(&world), &[Stroke::Decline]).await;
    assert!(matches!(state.stage(), Some(Stage::Field(_))));
    assert_eq!(legend(&state), NEW_RUN_AND_QUIT);
}

/// A field whose one run closed lists no run at a glance, so its legend names a new run and
/// quitting alone; `d`, which lists the closed too, brings the five keys back, and `d` again
/// takes them away.
#[tokio::test]
async fn a_field_whose_runs_closed_names_its_five_keys_once_d_lists_them() {
    let world = Bench::new();
    let dir = world.worktree.join(".yunta");
    std::fs::create_dir_all(&dir).expect("its .yunta");
    std::fs::write(dir.join("config.yaml"), "defaults:\n  isolation: none\n").expect("its config");
    world.run(FINISHES, NO_SESSIONS).await;
    let performer = performer_in(&world);
    let (opened, asked) = State::opened(now());
    let (mut state, _) = drive(opened, asked, &performer, &[]).await;
    assert!(matches!(state.stage(), Some(Stage::Field(_))));
    assert_eq!(
        legend(&state),
        NEW_RUN_AND_QUIT,
        "the closed run is not listed"
    );
    pressed(&mut state, &[Stroke::Insert('d')]);
    assert_eq!(legend(&state), FIVE_KEYS, "the closed run listed");
    pressed(&mut state, &[Stroke::Insert('d')]);
    assert_eq!(legend(&state), NEW_RUN_AND_QUIT);
}

/// A field that lists a run keeps the design's five keys, as its landed frames hold them.
#[tokio::test]
async fn a_field_with_a_run_in_sight_keeps_its_five_keys() {
    let (state, _world) = on_the_field(&[&FIRST]).await;
    assert_eq!(legend(&state), FIVE_KEYS);
}

/// The machine's field opened from the signpost, before its first reading lands, keeps the
/// keys of a field that lists runs: it is not yet a field with none.
#[tokio::test]
async fn a_field_not_read_yet_keeps_the_keys_of_one_that_lists_runs() {
    let root = tempfile::tempdir().expect("a root");
    let outside = root.path().join("elsewhere");
    std::fs::create_dir_all(&outside).expect("a directory in no repository");
    let performer = std::sync::Arc::new(Performer::new(
        workspace(&outside, root.path()),
        yunta_core::events::Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let (mut state, _) = drive(opened, asked, &performer, &[]).await;
    assert!(matches!(state.stage(), Some(Stage::Signpost)));
    pressed(&mut state, &[Stroke::Insert('e')]);
    assert!(matches!(state.stage(), Some(Stage::Field(_))));
    assert!(state.followed_runs().field.is_none(), "not read yet");
    assert_eq!(
        legend(&state),
        "j k move · enter open · x close · q quit",
        "no new run outside a repository"
    );
}
