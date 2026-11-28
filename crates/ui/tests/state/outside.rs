//! Outside a repository: the signpost stands, following nothing, and its palette reaches the
//! machine's field, which the application then follows; no run can be asked for there, and
//! going down from a page comes back to that field over the signpost.

use surco::components::Region;
use surco::doc::Block;
use surco::look::Look;
use surco::surface::{Markdown, Surface};
use yunta_api::{FieldReach, FollowNews, Followed, FollowedField, Repository};
use yunta_core::step::{NextStep, Reach};
use yunta_render::command;
use yunta_render::page::{FirstRequest, Signpost};
use yunta_render::screen::NEW_RUN;
use yunta_testkit_ui::sandbox::now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, Stage, State};

use super::{main_region, read_as};

/// Outside a repository the screen is the signpost as a page stands on it, the ways on from
/// here left to its keys and its palette, and its trail is yunta alone.
#[test]
fn outside_a_repository_the_screen_is_the_signpost() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let asked = state.update(Input::Outcome(*ticket, read_as(Repository::Absent)));
    assert!(asked.is_empty(), "{asked:?}");
    let page = Region::of_a_page(Signpost.on_screen());
    assert_eq!(main_region(&state), Markdown.draw(&page.doc));
    assert!(
        !main_region(&state).contains("yunta --help"),
        "{}",
        main_region(&state)
    );
    assert_eq!(state.screen(&Look::plain()).trail, ["yunta"]);
}

/// Outside a repository the application follows nothing; the signpost's palette offers every
/// run on this machine first, by the key the signpost takes it by, and taking that row stands
/// the field at the machine's reach, which the application then follows.
#[test]
fn outside_a_repository_it_follows_nothing_until_the_palette_reaches_the_machines_field() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    state.update(Input::Outcome(*ticket, read_as(Repository::Absent)));
    let nothing = Followed {
        field: None,
        run: None,
    };
    assert_eq!(state.followed(), nothing, "the signpost follows nothing");
    let opened = state.update(Input::Stroke(Stroke::Insert('/')));
    assert!(opened.is_empty(), "{opened:?}");
    let machine = NextStep::ListRuns {
        reach: Reach::Machine,
    };
    let overlay = state.screen(&Markdown.look()).overlay;
    let overlay = overlay.expect("the palette stands over the signpost");
    let commands: Vec<String> = overlay
        .doc
        .blocks()
        .iter()
        .find_map(|block| match block {
            Block::ChoiceList(list) => list.choices.as_ref(),
            _ => None,
        })
        .map(|choices| choices.rows.iter().filter_map(|row| row.said.clone()))
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(
        commands.first(),
        Some(&command::line(&machine)),
        "the machine's field is the signpost's first row: {commands:?}"
    );
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let machines = Followed {
        field: Some(FieldReach::Machine),
        run: None,
    };
    assert_eq!(state.followed(), machines);
    assert!(
        asked.contains(&Request::Follow(machines)),
        "the machine's field is followed: {asked:?}"
    );
}

/// Outside a repository no run can be asked for: the machine's field the signpost's `e`
/// opens draws no first row asking for one, its legend names no new run and `n` opens
/// nothing. A page opened over that field goes down to it with `q`, the signpost still
/// under it, and `q` there leaves.
#[test]
fn outside_a_repository_no_run_is_asked_for_and_q_goes_down_to_the_machines_field() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    state.update(Input::Outcome(*ticket, read_as(Repository::Absent)));
    state.update(Input::Stroke(Stroke::Insert('e')));
    let machines = FollowedField {
        runs: Vec::new(),
        unreadable: Vec::new(),
        elsewhere: 0,
        here: false,
    };
    state.update(Input::FollowNews(FollowNews::Field(Box::new(machines))));
    let region = main_region(&state).to_lowercase();
    assert!(
        !region.contains(&FirstRequest::QUESTION.to_lowercase()),
        "no first row asks for a run: {region}"
    );
    let legend = state.screen(&Look::plain()).legend;
    assert!(
        !legend.0.iter().any(|binding| binding.does == NEW_RUN),
        "the legend names no new run"
    );
    let asked = state.update(Input::Stroke(Stroke::Insert('n')));
    assert!(asked.is_empty(), "`n` asks nothing: {asked:?}");
    assert!(matches!(state.stage(), Some(Stage::Field(_))));

    state.update(Input::Stroke(Stroke::Insert('m')));
    assert!(matches!(state.stage(), Some(Stage::Doctor(_))));
    state.update(Input::Stroke(Stroke::Insert('q')));
    assert!(
        matches!(state.stage(), Some(Stage::Field(_))),
        "`q` goes down to the machine's field"
    );
    let under: Vec<&Stage> = state.stages_under().collect();
    assert!(
        matches!(under.as_slice(), [Stage::Signpost]),
        "the signpost stands under it still: {under:?}"
    );
    let left = state.update(Input::Stroke(Stroke::Insert('q')));
    assert!(left.contains(&Request::Leave), "{left:?}");
}

/// Outside a repository the field stands at the machine's reach alone: `e` asks to follow no
/// repository's field and leaves the field as it was, and neither help nor the palette names
/// it.
#[test]
fn outside_a_repository_the_fields_e_does_nothing_and_nothing_names_it() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    state.update(Input::Outcome(*ticket, read_as(Repository::Absent)));
    state.update(Input::Stroke(Stroke::Insert('e')));
    let machines = Followed {
        field: Some(FieldReach::Machine),
        run: None,
    };
    assert_eq!(state.followed(), machines);
    let asked = state.update(Input::Stroke(Stroke::Insert('e')));
    assert!(asked.is_empty(), "`e` asks nothing: {asked:?}");
    assert_eq!(state.followed(), machines, "the machine's field stands");

    state.update(Input::Stroke(Stroke::Insert('?')));
    let help = state.screen(&Markdown.look()).overlay.expect("help stands");
    let listed = Markdown.draw(&help.doc);
    assert!(!listed.contains("`e`"), "help names no `e`: {listed}");
    state.update(Input::Stroke(Stroke::Decline));
    state.update(Input::Stroke(Stroke::Insert('/')));
    let palette = state
        .screen(&Markdown.look())
        .overlay
        .expect("the palette stands");
    let keys: Vec<Option<String>> = palette
        .doc
        .blocks()
        .iter()
        .find_map(|block| match block {
            Block::ChoiceList(list) => list.choices.as_ref(),
            _ => None,
        })
        .map(|choices| choices.rows.iter().map(|row| row.key.clone()).collect())
        .unwrap_or_default();
    assert!(
        !keys.contains(&Some("e".to_string())),
        "the palette names no `e`: {keys:?}"
    );
}
