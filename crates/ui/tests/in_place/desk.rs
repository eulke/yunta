//! The plan's desk in place on a run's screen, through `State` alone, on the design's run at
//! the gate that shows its plan: it stands by itself with no key pressed, between the rule and
//! the attention row, on the plan's first task, the gate's options at its foot and its keys in
//! the screen's legend; a number names an option and records nothing until `enter` confirms
//! it, which records it on the run's log, once, and the note says what the run does now; an
//! option that asks for words opens its line at the desk's foot, in place of the options; the
//! arrows move what is read and never the task pointed at. A card that offers the plan it is
//! on turns to the plan's desk in place with `r`, `esc` there bringing the card back as it
//! was, and a number picked and confirmed at that desk records its option. `review` opens the
//! run's screen on the plan of the pause that stands, and on that one alone: a later gate
//! stands as its scene says. What is answered at a desk stays the desk until its pause goes.

use std::sync::Arc;

use surco::components::{Desk, FootRow, RailRow, ScreenLaid};
use surco::doc::Block;
use surco::glyphs::{Glyphs, Mark};
use surco::look::{Look, Width};
use surco::surface::{Markdown, Surface, Terminal};
use yunta_core::events::{Channel, EventPayload, GateEvent, GateResolvedPayload};
use yunta_core::step::NextStep;
use yunta_core::{OptionId, RunId};
use yunta_storage::Storage;
use yunta_testkit::workflows::{PLANNER_HANDS_TWO_TASKS, PLANNER_ON_THE_MOCK};
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_testkit_ui::screens::World;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Grounds, Input, Operation, Outcome, Performer, Request, Scene, State};

use super::{decided, help_rows, said_for};

/// The cells and the rows the desk takes on a terminal of 120 by 36: every cell inside the
/// screen's margins, and every row between the rule and the attention row.
const AT_120: (usize, usize) = (118, 32);

/// What a test reads of the desk standing in `screen`'s main region: the place its rail points
/// at, the title of its pane, the first line read in its pane, and how its question stands.
#[derive(Debug)]
struct ReadDesk {
    pointed: Vec<String>,
    titled: String,
    top: usize,
    foot: Foot,
    keys_of_its_own: bool,
}

/// How the desk's question stands, as a test reads it.
#[derive(Debug, PartialEq, Eq)]
enum Foot {
    /// Open, with the options a number names.
    Options(Vec<String>),
    /// The option a number named, waiting to be confirmed.
    Confirming(String),
    /// Words being typed for an option, and why the words given were not taken.
    Saying(Option<String>),
}

/// The desk standing in `screen`'s main region, as a test reads it: none where none stands.
fn desk(screen: &RunScreen) -> Option<ReadDesk> {
    let main = screen.state().screen(&Look::plain()).main;
    let [Block::Desk(desk)] = main.doc.blocks() else {
        return None;
    };
    Some(read(desk))
}

/// `desk` as a test reads it.
fn read(desk: &Desk) -> ReadDesk {
    let pointed = desk
        .rail
        .iter()
        .flat_map(|group| &group.rows)
        .filter_map(|row| match row {
            RailRow::Stop {
                name,
                pointed: true,
                ..
            } => Some(name.clone()),
            RailRow::Stop { .. } | RailRow::Tally { .. } | RailRow::Weighs(_) => None,
        })
        .collect();
    let foot = match &desk.foot.row {
        FootRow::Options { named, .. } => Foot::Options(named.clone()),
        FootRow::Confirming(option) => Foot::Confirming(option.name.clone()),
        FootRow::Saying { refused, .. } => Foot::Saying(refused.clone()),
    };
    ReadDesk {
        pointed,
        titled: desk.pane.title.clone(),
        top: desk.pane.sight.top,
        foot,
        keys_of_its_own: !desk.foot.legend.is_empty(),
    }
}

/// `screen`'s main region laid out where a terminal of 120 by 36 lays it, and handed back as
/// a terminal hands it, as the next key moves from.
fn laid_out(screen: &mut RunScreen) {
    let (cells, rows) = AT_120;
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(cells),
        ..Look::plain()
    };
    let main = screen.state().screen(&look).main;
    let paned = Terminal::on(look).pane(&main, rows);
    screen.laid(ScreenLaid {
        main: paned.laid,
        side: None,
        overlay: None,
    });
}

#[tokio::test]
async fn the_desk_stands_by_itself_on_the_plans_first_task_with_its_keys_in_the_legend() {
    let screen = run::opened("desk").await;
    assert!(matches!(screen.state().scene(), Some(Scene::Desk)));
    let desk = desk(&screen).expect("the desk takes the main region");
    assert_eq!(desk.pointed, ["global-pack-install"]);
    assert_eq!(desk.titled, "global-pack-install");
    assert_eq!(
        desk.foot,
        Foot::Options(vec!["approve".into(), "adjust".into(), "abort".into()])
    );
    assert!(
        !desk.keys_of_its_own,
        "its keys stand in the screen's legend"
    );
    let drawn = screen.state().screen(&Look::plain());
    assert!(drawn.about.is_none(), "nothing stands across the desk");
    assert!(drawn.side.is_none(), "the desk takes the nodes' place too");
    assert!(drawn.meter.is_none(), "the desk reaches the attention row");
    assert!(
        drawn.attention.is_none(),
        "the run's decision stands in sight"
    );
    let legend = drawn.legend.spelled(Glyphs::Unicode);
    assert!(
        legend.starts_with("1 2 3 answer · tab next place"),
        "{legend}"
    );
}

#[tokio::test]
async fn a_number_waits_for_enter_which_records_its_option_once_with_what_the_run_does_now() {
    let mut screen = run::opened("desk").await;
    let before = decided(&screen);
    screen.pressed(&[Stroke::Insert('1')]).await;
    let desk = self::desk(&screen).expect("the desk stands");
    assert_eq!(desk.foot, Foot::Confirming("approve".into()));
    assert_eq!(decided(&screen), before, "a number records nothing");
    screen.pressed(&[Stroke::Enter]).await;
    let recorded = decided(&screen);
    assert_eq!(recorded[..before.len()], before);
    assert_eq!(recorded[before.len()..], ["approve"], "recorded once");
    let noted = screen.state().screen(&Look::plain()).noted;
    let noted = noted.expect("the answer recorded is noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(noted.said, "Recorded: approve. On to `implement`.");
}

/// The words given with each decision on `screen`'s run's log, in the log's order.
fn said_with(screen: &RunScreen) -> Vec<Option<String>> {
    let run: RunId = screen.state().followed().run.expect("a run's screen");
    let storage = Storage::open(&screen.store()).expect("the cut's log");
    let events = storage.events_for_run(&run).expect("the run's log");
    events
        .iter()
        .filter_map(|event| match event.payload()? {
            EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(chosen))) => {
                Some(chosen.free_text.clone())
            }
            _ => None,
        })
        .collect()
}

/// `2` then `enter` opens the line of `adjust` at the desk's foot, in place of the options,
/// the desk standing and the run's decision in sight, every key typing there; `esc` brings the
/// options back.
#[tokio::test]
async fn an_option_that_asks_for_words_opens_its_line_at_the_desks_foot() {
    let mut screen = run::opened("desk").await;
    screen.pressed(&[Stroke::Insert('2'), Stroke::Enter]).await;
    assert!(matches!(screen.state().scene(), Some(Scene::Desk)));
    let desk = self::desk(&screen).expect("the desk stands");
    assert_eq!(desk.foot, Foot::Saying(None), "in place of the options");
    assert!(screen.state().typing(), "every key types on the line");
    let drawn = screen.state().screen(&Look::plain());
    assert!(
        drawn.attention.is_none(),
        "the run's decision stands in sight"
    );
    assert_eq!(
        drawn.legend.spelled(Glyphs::Unicode),
        "enter send it · esc go back"
    );
    screen.pressed(&[Stroke::Decline]).await;
    let desk = self::desk(&screen).expect("the desk stands");
    assert_eq!(
        desk.foot,
        Foot::Options(vec!["approve".into(), "adjust".into(), "abort".into()]),
        "esc brings the options back"
    );
    assert!(!screen.state().typing());
}

/// An empty line at the desk's foot is refused under it with nothing recorded; words typed
/// and `enter` record `adjust` with them, once, and the note says where the words went.
#[tokio::test]
async fn the_words_typed_at_the_desks_foot_are_recorded_with_their_option() {
    let mut screen = run::opened("desk").await;
    let before = decided(&screen);
    let enter = Stroke::Enter;
    screen.pressed(&[Stroke::Insert('2'), enter, enter]).await;
    assert_eq!(decided(&screen), before, "an empty line records nothing");
    let refused = Some("this option needs an answer".to_string());
    let desk = self::desk(&screen).expect("the desk stands");
    assert_eq!(desk.foot, Foot::Saying(refused));
    let words: Vec<Stroke> = "split the second task"
        .chars()
        .map(Stroke::Insert)
        .collect();
    screen.pressed(&words).await;
    screen.pressed(&[Stroke::Enter]).await;
    let recorded = decided(&screen);
    assert_eq!(recorded[..before.len()], before);
    assert_eq!(recorded[before.len()..], ["adjust"], "recorded once");
    let said = said_with(&screen);
    let words = Some("split the second task".to_string());
    assert_eq!(said.last(), Some(&words));
    let noted = screen.state().screen(&Look::plain()).noted;
    let noted = noted.expect("the answer recorded is noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(
        noted.said,
        "Sent back to `plan` with your words: \"split the second task\"."
    );
}

#[tokio::test]
async fn the_arrows_move_what_is_read_and_never_the_task_pointed_at() {
    let mut screen = run::opened("desk").await;
    laid_out(&mut screen);
    let before = desk(&screen).expect("the desk stands");
    assert_eq!(before.top, 0, "the pane opens at its first line");
    screen.pressed(&[Stroke::Down]).await;
    let after = desk(&screen).expect("the desk still stands");
    assert_eq!(after.top, 1, "down reads one line on");
    assert_eq!(after.pointed, before.pointed);
    assert_eq!(after.titled, before.titled);
    assert_eq!(after.foot, before.foot, "nothing is chosen by the arrows");
    assert!(matches!(screen.state().scene(), Some(Scene::Desk)));
}

/// Whether `state`'s main region holds a card at its foot.
fn ends_with_a_card(state: &State) -> bool {
    let main = state.screen(&Look::plain()).main;
    matches!(main.doc.blocks().last(), Some(Block::Card(_)))
}

/// Whether `state`'s main region holds the plan's desk alone.
fn holds_the_desk_alone(state: &State) -> bool {
    let main = state.screen(&Look::plain()).main;
    matches!(main.doc.blocks(), [Block::Desk(_)])
}

/// `state`'s main region, as Markdown draws it.
fn main_drawn(state: &State) -> String {
    Markdown.draw(&state.screen(&Look::plain()).main.doc)
}

/// A run parked on its plan's gate stands as the gate's card in place, which offers the plan:
/// `r` turns to the plan's desk in place, taking the screen, and `esc` there brings the card
/// back with its pointer where it was left; at the desk read again, a number picked and
/// confirmed records that option.
#[tokio::test]
async fn r_turns_a_card_to_its_plans_desk_in_place_and_a_number_picked_there_decides() {
    let world = World::made().await;
    let mut state = world.driven(&[Stroke::Enter]).await;
    let on_the_card = |state: &State| {
        matches!(state.scene(), Some(Scene::Card(_, Grounds::Shown))) && ends_with_a_card(state)
    };
    assert!(on_the_card(&state), "the card stands first");
    state.update(Input::Stroke(Stroke::Insert('j')));
    let card = main_drawn(&state);

    state.update(Input::Stroke(Stroke::Insert('r')));
    assert!(matches!(state.scene(), Some(Scene::Desk)));
    assert!(holds_the_desk_alone(&state), "the desk takes the screen");
    let legend = state.screen(&Look::plain()).legend;
    let spelled = legend.spelled(Glyphs::Unicode);
    assert!(spelled.ends_with("esc go back"), "{spelled}");
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(asked.is_empty(), "{asked:?}");
    assert!(on_the_card(&state), "esc brings the card back");
    assert_eq!(main_drawn(&state), card, "as it was left");

    state.update(Input::Stroke(Stroke::Insert('r')));
    state.update(Input::Stroke(Stroke::Insert('1')));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(
        _,
        Operation::Decide {
            run: decided,
            option,
            words,
        },
    )] = asked.as_slice()
    else {
        panic!("the option is recorded: {asked:?}");
    };
    assert_eq!(decided, world.parked());
    assert_eq!(option, &OptionId::from("approve"));
    assert_eq!(words, &None);
}

/// A plan, a gate that shows it, and a second gate that shows the plan again over the run's
/// change as it stands then.
const TWO_GATES_ON_ONE_PLAN: &str = r#"
name: twice
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
  - id: ship
    kind: gate
    assignee: lead
    message: "Ship it?"
    options: [go]
    shows: [{ node: plan, kind: tasks }]
"#;

/// `review` opens the run's screen on the plan of the pause that stands, and spends that on
/// it: the plan answered and a file made before the run goes on, the later gate that shows
/// the plan over a change that holds files stands as its card.
#[tokio::test]
async fn review_opens_on_the_plan_of_its_pause_alone_and_a_later_gate_stands_as_its_card() {
    let bench = Bench::with_run_id("01M7REV1EWTW1CE00000000000");
    let config = bench.worktree.join(".yunta/config.yaml");
    std::fs::create_dir_all(config.parent().expect("its .yunta")).expect("its .yunta");
    std::fs::write(&config, PLANNER_ON_THE_MOCK).expect("its config");
    bench
        .run(TWO_GATES_ON_ONE_PLAN, PLANNER_HANDS_TWO_TASKS)
        .await;
    let root = bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Arc::new(Performer::new(
        workspace(&bench.worktree, root),
        Channel::Tty,
    ));
    let review = NextStep::Review {
        run: bench.run_id.clone(),
    };
    let (opened, asked) = State::opened_on(now(), &review).expect("a stage takes the step");
    let (state, _) = drive(opened, asked, &performer, &[]).await;
    assert!(
        matches!(state.scene(), Some(Scene::Desk)),
        "review opens on the plan: {:?}",
        state.scene()
    );

    bench
        .answer_parked("approve", None)
        .await
        .expect("the plan answered");
    std::fs::write(bench.worktree.join("made.txt"), "made\n").expect("a file made");
    bench.wake().await;
    let (state, _) = drive(state, Vec::new(), &performer, &[]).await;
    assert!(
        matches!(state.scene(), Some(Scene::Card(_, Grounds::Shown))),
        "the later gate stands as its card: {:?}",
        state.scene()
    );
}

/// On `review` of a run whose gate counted a changed file, what is answered at the plan's
/// desk stays the desk while it is recorded, and once it is, until its pause goes.
#[tokio::test]
async fn an_answer_at_the_desk_keeps_the_desk_until_its_pause_goes() {
    let world = World::made().await;
    let review = NextStep::Review {
        run: world.parked().clone(),
    };
    let mut state = world.driven_from(&review, &[]).await;
    assert!(matches!(state.scene(), Some(Scene::Desk)));
    state.update(Input::Stroke(Stroke::Insert('1')));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let decide = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::Decide { run, .. }) => Some((*ticket, run.clone())),
        _ => None,
    });
    let (ticket, run) = decide.unwrap_or_else(|| panic!("the option is recorded: {asked:?}"));
    assert!(
        matches!(state.scene(), Some(Scene::Desk)) && holds_the_desk_alone(&state),
        "the answer in flight stays the desk: {:?}",
        state.scene()
    );
    state.update(Input::Outcome(ticket, Outcome::Decided { run }));
    assert!(
        matches!(state.scene(), Some(Scene::Desk)) && holds_the_desk_alone(&state),
        "recorded, its pause still standing: {:?}",
        state.scene()
    );
    let noted = state.screen(&Look::plain()).noted;
    let noted = noted.expect("the answer recorded is noted");
    assert!(
        noted.said.starts_with("Recorded: approve."),
        "{}",
        noted.said
    );
}

/// Help over the plan's desk says every key the desk takes in the desk's words: those its
/// legend names, and those it leaves to help, `j k` to the next place as `tab` goes, the
/// place before and a page at a time; no row of the run's stands for any of them, none
/// saying `j k` move along the nodes.
#[tokio::test]
async fn help_over_the_desk_says_every_key_it_takes_and_no_run_row_for_them() {
    let mut screen = run::opened("desk").await;
    screen.pressed(&[Stroke::Insert('?')]).await;
    let rows = help_rows(&mut screen);
    for key in ["j", "k", "tab"] {
        assert_eq!(said_for(&rows, key), ["next place"], "{key}: {rows:#?}");
    }
    assert_eq!(said_for(&rows, "shift-tab"), ["place before"], "{rows:#?}");
    assert_eq!(said_for(&rows, "pgup"), ["by the page"], "{rows:#?}");
    assert_eq!(said_for(&rows, "1"), ["answer"], "{rows:#?}");
    assert_eq!(said_for(&rows, "esc"), ["leave it waiting"], "{rows:#?}");
}
