//! The keys every stage answers to where nothing is typed and no prompt stands, pressed on
//! each state the walk reaches. `n` opens a new run where one can be asked for, but where the
//! stage binds `n` itself (the workflows' new workflow, the packs' new pack: a legend naming
//! it for another act); where none can be (outside a repository, in one never set up) it
//! opens nothing and the legend names no new run. `q` on a page goes down to the nearest
//! field under it, else to the stage at the bottom of what stands, building none and
//! dropping none; the field, a setup and the signpost leave with it. A line typed takes both
//! as letters, a prompt in place and a box over the screen as their own keys.

use std::panic::{catch_unwind, AssertUnwindSafe};

use surco::components::Screen;
use yunta_render::screen::NEW_RUN;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Stage, State};

use crate::walk::classes::stage_kind;
use crate::walk::Reached;

/// `n` and `q` pressed on `state`, whose screen as Markdown lays it out is `screen`, reached as
/// `reached` says, where nothing is typed and no prompt or box stands: `n` opens a new run where
/// one can be asked for and the stage does not bind it, and opens nothing where none can, its
/// legend naming none; `q` on a page goes down to the nearest field under it, else to the
/// stage at the bottom.
pub(super) fn keys_everywhere_hold(state: &State, screen: &Screen, reached: &Reached<'_>) {
    let Some(stage) = state.stage() else {
        return;
    };
    let typed = state.typing() || screen.typed;
    let prompt =
        state.dialog().is_some() || state.scene().is_some_and(|scene| scene.asks_in_place());
    if typed || prompt {
        return;
    }
    let pressing_n = || reached.then("Stroke(Insert('n'))");
    match state.followed_runs().can_ask_for_a_run() {
        true if !binds_n(screen) => {
            let came = pressed(state, 'n', reached);
            assert!(
                matches!(came.stage(), Some(Stage::Start(_))),
                "`n` opens no new run, after:\n{}",
                pressing_n().inputs()
            );
        }
        true => {}
        false => {
            assert!(
                !names_a_new_run(screen),
                "the legend names a new run where none can be asked for, reached by:\n{}",
                reached.inputs()
            );
            let came = pressed(state, 'n', reached);
            assert!(
                !matches!(came.stage(), Some(Stage::Start(_))),
                "`n` opens a new run where none can be asked for, after:\n{}",
                pressing_n().inputs()
            );
        }
    }
    let leaves = matches!(stage, Stage::Field(_) | Stage::Setup(_) | Stage::Signpost);
    if !leaves {
        let came = pressed(state, 'q', reached);
        assert!(
            went_down(state, &came),
            "`q` on a page goes to the nearest field under it, else to the bottom stage, and \
             came to {:?} over {:?}, after:\n{}",
            came.stage().map(stage_kind),
            came.stages_under().map(stage_kind).collect::<Vec<_>>(),
            reached.then("Stroke(Insert('q'))").inputs()
        );
    }
}

/// Whether `came`, `state` after `q`, stands where going down from `state` leads: on the
/// nearest field under the stage `state` is at, the stages under that field as they were;
/// where no field stands under it, on the stage at the bottom, alone, or on the field in the
/// first request's place, which gives way to it once the field holds a run. Where `state`
/// stands at the bottom, nowhere else.
fn went_down(state: &State, came: &State) -> bool {
    let under: Vec<&Stage> = state.stages_under().collect();
    let came_under: Vec<&Stage> = came.stages_under().collect();
    let Some(came_to) = came.stage() else {
        return false;
    };
    let field = under
        .iter()
        .rposition(|stage| matches!(stage, Stage::Field(_)));
    match (field, under.first()) {
        (Some(at), _) => {
            let kept = came_under.iter().map(|stage| stage_kind(stage));
            let below = under[..at].iter().map(|stage| stage_kind(stage));
            matches!(came_to, Stage::Field(_)) && kept.eq(below)
        }
        (None, Some(bottom)) => {
            let given_way =
                matches!(bottom, Stage::FirstRun(_)) && matches!(came_to, Stage::Field(_));
            came_under.is_empty() && (stage_kind(came_to) == stage_kind(bottom) || given_way)
        }
        (None, None) => state
            .stage()
            .is_some_and(|stage| stage_kind(stage) == stage_kind(came_to)),
    }
}

/// Whether `screen`'s legend names `n` for an act of the stage's own, not a new run.
fn binds_n(screen: &Screen) -> bool {
    screen
        .legend
        .0
        .iter()
        .any(|binding| binding.keys.named() == "n" && binding.does != NEW_RUN)
}

/// Whether `screen`'s legend names a new run, by any key.
fn names_a_new_run(screen: &Screen) -> bool {
    screen
        .legend
        .0
        .iter()
        .any(|binding| binding.does == NEW_RUN)
}

/// A copy of `state`, reached as `reached` says, with `key` pressed on it.
fn pressed(state: &State, key: char, reached: &Reached<'_>) -> State {
    let mut pressed = state.clone();
    let stroke = Input::Stroke(Stroke::Insert(key));
    catch_unwind(AssertUnwindSafe(|| pressed.update(stroke))).unwrap_or_else(|_| {
        panic!(
            "`{key}` panicked, after:\n{}",
            reached.then(format!("Stroke(Insert('{key}'))")).inputs()
        )
    });
    pressed
}
