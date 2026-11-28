//! See [`super`]. The pack a setup installs, read before it is: `enter` on the row that writes,
//! where fragua was fetched, shows its audit as `yunta pack add` shows it, a page the arrows
//! scroll and `esc` leaves for the rows; `enter` there stands its confirmation, which names
//! what writing touches and the commands that do the same at a shell, `yunta init` where no row
//! was changed and `yunta init -i` where one was, and holds through what the setup follows.
//! Confirmed, the setup is written with fragua installed first; `esc` writes it without
//! fragua, as `init` writes it with the same answers. Once written, a note says
//! what writing touched, and `enter` goes on to the first request.

use std::path::Path;
use std::sync::Arc;

use surco::components::{Laid, ScreenLaid, Tally};
use surco::glyphs::{Glyphs, Mark};
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::{Markdown, Surface, Terminal};
use surco::words::Standing;
use yunta_api::{ProjectDraft, SetupPack, Stop, Workspace};
use yunta_core::events::Channel;
use yunta_core::step::{Arg, NextStep};
use yunta_core::RunId;
use yunta_render::command;
use yunta_testkit::reference_pack::ReferencePack;
use yunta_testkit::workflows::GATED;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace_reaching};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, InstallsThePack, Operation, Performer, Request, State};

use super::{main_region, never_set_up, pressed, pressing, reaching, typed};

/// What the pack a setup installs is called.
const FRAGUA: &str = "yunta/fragua";

/// What the confirmation says writing with fragua does, as the paths a setup writes name what
/// it touches.
const WRITES_WITH_FRAGUA: &str = "It writes .yunta/config.yaml and installs yunta/fragua. \
                                  Nothing outside .yunta/ is touched but .gitignore.";

/// A mock fixture with no sessions: the run a test parks opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The run of the repository being set up that comes to wait on the person while the
/// confirmation stands.
const COMES_WAITING: RunId = RunId::from_static("01M7SETVPC0NF1RMHAVESNEWS0");

/// The rows the terminal gives the audit: fewer than its page, so it scrolls.
const ROWS: usize = 4;

/// The setup of a repository never set up under `root`, whose git points the Yunta
/// repository's URL at the local fragua, Claude Code answering for no one: its rows stand,
/// fragua fetched to install as a pack. With the performer that carries its work out, and the
/// workspace it works in.
async fn on_the_rows_with_fragua(root: &Path) -> (State, Arc<Performer>, Workspace) {
    let reference = ReferencePack::made(&root.join("upstream/yunta"));
    let repo = never_set_up(root);
    let workspace = reaching(&repo, root, reference.reached());
    let performer = Arc::new(Performer::new(workspace.clone(), Channel::Tty));
    let (opened, asked) = State::opened(now());
    let (state, surfaced) = drive(opened, asked, &performer, &[]).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let region = main_region(&state);
    assert!(region.contains("to install as a pack"), "{region}");
    (state, performer, workspace)
}

/// What the confirmation standing over `state` says under its question, as Markdown writes
/// it, and its question; none where no confirmation stands.
fn confirmation(state: &State) -> Option<(String, String)> {
    let overlay = state.screen(&Markdown.look()).overlay?;
    overlay
        .confirms
        .then(|| (overlay.title.clone(), Markdown.draw(&overlay.doc)))
}

/// Whether the confirmation's words `says` name `step` on a row of its own.
fn names_alone(says: &str, step: &NextStep) -> bool {
    let line = command::line(step);
    says.lines()
        .any(|row| row.trim_matches(['-', '`', ' ']) == line)
}

/// `state`, on the rows with fragua fetched, its base typed in place as `trunk`, then `enter`
/// on the row that writes and on fragua's audit: its confirmation stands.
fn confirming_on_trunk(state: &mut State) {
    // Up to the name, down to the base, typed in place; then down to the row that writes.
    pressing(state, &[Stroke::Insert('k'); 8]);
    pressing(state, &[Stroke::Insert('j'), Stroke::Enter]);
    assert!(typed(state, "trunk").is_empty());
    pressing(state, &[Stroke::Insert('j'); 8]);
    pressing(state, &[Stroke::Enter, Stroke::Enter]);
}

/// `enter` on the row that writes, where fragua was fetched, shows fragua's audit as
/// `yunta pack add` shows it before it installs, the pack named on the trail, and asks no work;
/// `enter` on the audit stands its confirmation, asking no work either: the pack named in its
/// question, what writing with it touches, and `yunta pack add` with the source the setup
/// fetched it from and `yunta init`, each alone on its row, the commands that do the same.
#[tokio::test]
async fn enter_on_the_write_row_shows_fraguas_audit_then_its_confirmation_asking_no_work() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (mut state, _performer, _) = on_the_rows_with_fragua(root.path()).await;
    pressed(&mut state, Stroke::Enter);
    let screen = state.screen(&Markdown.look());
    assert_eq!(screen.trail.last().map(String::as_str), Some(FRAGUA));
    let page = Markdown.draw(&screen.main.doc);
    assert!(
        page.contains("pack yunta/fragua") && page.contains("installing"),
        "{page}"
    );
    assert!(page.contains(".yunta/workflows/fragua.yaml"), "{page}");
    assert!(page.contains("grill"), "the audit reads each node: {page}");
    assert!(confirmation(&state).is_none(), "the audit is read first");
    pressed(&mut state, Stroke::Enter);
    let (asks, says) = confirmation(&state).expect("a confirmation stands over the audit");
    assert_eq!(asks, format!("install {FRAGUA}?"));
    assert!(says.contains(WRITES_WITH_FRAGUA), "{says}");
    let fetched_from = NextStep::PackAdd {
        source: Arg::Given(SetupPack::of_this_binary().source().to_string()),
    };
    for same in [fetched_from, NextStep::Init { interactive: false }] {
        let alone = names_alone(&says, &same);
        assert!(
            alone,
            "`{}` alone on its row:\n{says}",
            command::line(&same)
        );
    }
}

/// Where a row was changed, `yunta init` would not write what the setup writes, so the
/// confirmation names `yunta init -i`, which asks for what the rows hold, and not `yunta init`.
#[tokio::test]
async fn a_confirmation_over_a_row_changed_names_init_asking() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (mut state, _performer, _) = on_the_rows_with_fragua(root.path()).await;
    confirming_on_trunk(&mut state);
    let (_, says) = confirmation(&state).expect("a confirmation stands over the audit");
    let asking = NextStep::Init { interactive: true };
    assert!(names_alone(&says, &asking), "{says}");
    let unasked = NextStep::Init { interactive: false };
    assert!(!names_alone(&says, &unasked), "{says}");
}

/// The setup a repository's draft makes, its base typed as `trunk` and nothing else answered.
fn on_trunk(draft: &ProjectDraft) -> yunta_api::ProjectSetup {
    draft.answered("", "trunk", Vec::new())
}

/// With its base typed in place, `enter` on fragua's confirmation asks the setup the rows make
/// written with fragua installed, fragua's roles proposed; `esc` asks it written without
/// fragua, as `init` writes it with the same answers, the repository's own role proposed.
/// Either way the confirmation is answered.
#[tokio::test]
async fn enter_writes_the_setup_with_fragua_and_esc_without_it() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (mut state, _performer, workspace) = on_the_rows_with_fragua(root.path()).await;
    confirming_on_trunk(&mut state);
    let mut declined = state.clone();
    let stop = Stop::new();
    let detected = workspace.detect_project(false, &stop).await;
    let draft = detected.expect("what setting it up finds").draft;
    let (packs, source) = (workspace.packs(), SetupPack::of_this_binary());
    let fetched = packs.fetch(source.source(), false, &stop).await;
    let proposed = fetched.expect("fragua, fetched again").proposed();
    let with_fragua = ProjectDraft {
        proposed,
        ..draft.clone()
    };
    for (state, stroke, installs, draft) in [
        (
            &mut state,
            Stroke::Enter,
            InstallsThePack::Yes,
            &with_fragua,
        ),
        (&mut declined, Stroke::Decline, InstallsThePack::No, &draft),
    ] {
        let asked = state.update(Input::Stroke(stroke));
        let [Request::Perform(_, Operation::WriteSetup { setup, pack, .. })] = asked.as_slice()
        else {
            panic!("{stroke:?} on the confirmation writes the setup: {asked:?}");
        };
        assert_eq!(*pack, installs, "{stroke:?}");
        assert_eq!(*setup, on_trunk(draft), "{stroke:?}");
        assert!(
            confirmation(state).is_none(),
            "the confirmation is answered"
        );
    }
}

/// The terminal the audit is shown on: 80 cells, in Unicode glyphs, unpainted.
fn terminal_look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(80),
        motion: Motion::Still,
    }
}

/// `state`'s main region shown in [`ROWS`] rows, and what was laid out of it handed back as a
/// terminal hands it back.
fn laid_out(state: &mut State) {
    let screen = state.screen(&terminal_look());
    let main: Laid = Terminal::on(terminal_look()).pane(&screen.main, ROWS).laid;
    let laid = ScreenLaid {
        main,
        side: None,
        overlay: None,
    };
    assert!(state.update(Input::Laid(laid)).is_empty());
}

/// The audit scrolls by the arrows its legend names, each from what a terminal laid out of it
/// last, and `esc` gives the rows back as they stood, asking nothing.
#[tokio::test]
async fn the_audit_scrolls_and_escape_gives_the_rows_back() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (mut state, _performer, _) = on_the_rows_with_fragua(root.path()).await;
    let rows = main_region(&state);
    pressed(&mut state, Stroke::Enter);
    laid_out(&mut state);
    let top = |state: &State| state.screen(&terminal_look()).main.sight.top;
    assert_eq!(top(&state), 0);
    for stroke in [Stroke::Down, Stroke::Down] {
        pressed(&mut state, stroke);
        laid_out(&mut state);
    }
    assert_eq!(top(&state), 2, "the arrows scroll the audit");
    pressed(&mut state, Stroke::Up);
    assert_eq!(top(&state), 1);
    pressed(&mut state, Stroke::Decline);
    let screen = state.screen(&Markdown.look());
    assert_eq!(screen.trail.last().map(String::as_str), Some("set up"));
    assert_eq!(main_region(&state), rows, "the rows as they stood");
}

/// A confirmation holds what it was asked through what the setup follows: a run of the
/// repository that comes to wait on the person while it stands is counted on the tally, and
/// the confirmation stands as it was asked, `enter` writing the setup with fragua.
#[tokio::test]
async fn the_confirmation_holds_through_followed_news() {
    let world = Bench::with_run_id(COMES_WAITING.as_str());
    let root = world
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let reference = ReferencePack::made(&root.join("upstream/yunta"));
    let workspace = workspace_reaching(&world.worktree, root, Vec::new(), reference.reached());
    let performer = Arc::new(Performer::new(workspace, Channel::Tty));
    let (opened, asked) = State::opened(now());
    let to_the_confirmation = [Stroke::Enter, Stroke::Enter];
    let (state, _) = drive(opened, asked, &performer, &to_the_confirmation).await;
    let asked = confirmation(&state).map(|(asks, _)| asks);
    assert_eq!(asked.as_deref(), Some("install yunta/fragua?"));
    let needing = |state: &State| match state.screen(&Markdown.look()).tally {
        Tally::Counted(counts) => counts
            .iter()
            .filter(|count| count.standing == Standing::NeedsYou)
            .map(|count| count.runs)
            .sum(),
        Tally::Uncounted => 0,
    };
    assert_eq!(needing(&state), 0);
    world.run(GATED, NO_SESSIONS).await;
    let (mut state, _) = drive(state, Vec::new(), &performer, &[]).await;
    assert_eq!(needing(&state), 1, "the run that came is followed");
    assert_eq!(confirmation(&state).map(|(asks, _)| asks), asked);
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(_, Operation::WriteSetup { pack, .. })] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(*pack, InstallsThePack::Yes);
}

/// Written with fragua confirmed, the setup shows the page `yunta pack add` ends with and notes
/// what writing touched, `.gitignore` among it; `enter` goes on to the first request. Written
/// without fragua, in a repository whose `.gitignore` holds yunta's lines already, it shows
/// `init`'s page and notes that nothing outside `.yunta/` was touched.
#[tokio::test]
async fn the_setup_written_notes_what_it_touched_and_goes_on_to_the_first_request() {
    let cases = [
        (
            Stroke::Enter,
            None,
            "pack yunta/fragua",
            "Wrote .yunta/config.yaml and installed yunta/fragua. Nothing outside .yunta/ was \
             touched but .gitignore.",
        ),
        (
            Stroke::Decline,
            Some("# added by `yunta init`\n.yunta/runs/\n"),
            "for this repo's CLAUDE.md",
            "Wrote .yunta/config.yaml. Nothing outside .yunta/ was touched.",
        ),
    ];
    for (answer, ignored, page, note) in cases {
        let root = tempfile::tempdir().expect("a temporary root");
        let repo = root.path().join("project");
        if let Some(ignored) = ignored {
            std::fs::create_dir_all(&repo).expect("the repository's directory");
            std::fs::write(repo.join(".gitignore"), ignored).expect("an ignore file");
        }
        let (state, performer, _) = on_the_rows_with_fragua(root.path()).await;
        let strokes = [Stroke::Enter, Stroke::Enter, answer];
        let (mut state, surfaced) = drive(state, Vec::new(), &performer, &strokes).await;
        assert!(surfaced.is_empty(), "{surfaced:?}");
        let screen = state.screen(&Markdown.look());
        assert_eq!(screen.trail.last().map(String::as_str), Some("set up"));
        let region = Markdown.draw(&screen.main.doc);
        assert!(region.contains(page), "{answer:?}: {region}");
        let noted = screen.noted.expect("a note says what writing touched");
        assert_eq!(noted.mark, Mark::Done);
        assert_eq!(noted.said, note, "{answer:?}");
        assert_eq!(
            repo.join(".yunta/yunta.lock").exists(),
            answer == Stroke::Enter,
            "{answer:?}: fragua's lock"
        );
        let asked = state.update(Input::Stroke(Stroke::Enter));
        let [Request::Perform(_, Operation::ReadCatalog)] = asked.as_slice() else {
            panic!("`enter` on the setup written goes on to the first request: {asked:?}");
        };
    }
}
