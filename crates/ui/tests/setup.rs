//! Setting up, in the application, a repository Yunta was never set up in: the rows changed
//! in place become the setup `init -i` writes from the same answers, one `enter` on the row
//! that writes, where the pointer starts, writes what `init` writes unasked, every role on
//! each adapter that answers in the order they were found and its model left to each, and a
//! setup left before it is written writes nothing and lets the project it detected go. Where
//! the pack a setup installs was fetched, its audit and its confirmation stand before the
//! write (`audit`). The setup follows the field of the repository's runs, as the first request
//! does: its tally and its attention row say what those runs need, and its sentence opens on
//! what the field holds, once the field is read.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::look::{Look, Width};
use surco::surface::{Markdown, Surface};
use tokio::sync::mpsc;
use yunta_api::{
    Committer, FieldReach, Followed, ProjectDraft, ProposedRunners, Repository, RunnerAnswer,
    Workspace,
};
use yunta_core::events::Channel;
use yunta_core::{AdapterId, Detected, ModelName, NonEmpty, ProbedAdapter, RunnerName};
use yunta_testkit::reference_pack;
use yunta_testkit_ui::driving::{carried_out, drive};
use yunta_testkit_ui::sandbox::{
    claude_answering, codex_answering, now, workspace_on, workspace_reaching,
};
use yunta_ui::prompt::Stroke;
use yunta_ui::{
    Input, InstallsThePack, Operation, Outcome, Performer, RepositoryReading, Request, State,
    Ticket,
};

mod support;

#[path = "setup/audit.rs"]
mod audit;
#[path = "setup/field.rs"]
mod field;
#[path = "setup/work.rs"]
mod work;

/// What setting a repository up found: the name `project` and the base branch `main`,
/// Claude Code answering and Codex not, and two runners proposed.
fn draft() -> ProjectDraft {
    ProjectDraft {
        repo: PathBuf::from("/home/me/project"),
        name: "project".to_string(),
        base_branch: "main".to_string(),
        detected: Detected::default(),
        probed: vec![
            ProbedAdapter {
                id: AdapterId::from_static("claude-code"),
                healthy: true,
                detail: "2.1.0".to_string(),
            },
            ProbedAdapter {
                id: AdapterId::from_static("codex"),
                healthy: false,
                detail: "not on PATH".to_string(),
            },
        ],
        proposed: ProposedRunners::Named {
            roles: NonEmpty::from((
                RunnerName::from("executor"),
                vec![RunnerName::from("reviewer")],
            )),
            needs_default: false,
        },
    }
}

/// The application opened in a repository never set up, and the ticket the work that
/// finds what it runs went under.
fn on_the_setup() -> (State, Ticket) {
    let (state, detecting, _) = on_the_setup_fetching();
    (state, detecting)
}

/// The application opened in a repository never set up, and the tickets the work that finds
/// what it runs and the fetch of the pack a setup installs went under, both asked as the setup
/// opens.
fn on_the_setup_fetching() -> (State, Ticket, Ticket) {
    let (mut state, asked) = State::opened(now());
    let [Request::Perform(opening, Operation::ReadRepository)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let never_set_up = Outcome::Repository(Box::new(RepositoryReading {
        repository: Repository::NeverSetUp {
            dir: PathBuf::from("/home/me/repo"),
        },
        storage: PathBuf::from("/home/me/.yunta/yunta.db"),
        prices: None,
        home: None,
    }));
    let asked = state.update(Input::Outcome(*opening, never_set_up));
    let [Request::Perform(detecting, Operation::DetectProject), Request::Perform(fetching, Operation::FetchSetupPack), Request::Follow(followed)] =
        asked.as_slice()
    else {
        panic!(
            "a repository never set up opens on finding what it runs and fetching its pack, \
             following its field: {asked:?}"
        );
    };
    let here = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    assert_eq!(*followed, here);
    (state, *detecting, *fetching)
}

/// `draft` handed to `state` as what the work under `detecting` found, git naming no one
/// committing here: the state asks nothing more, its pack's fetch asked as it opened.
fn found_by_hand(state: &mut State, detecting: Ticket, draft: ProjectDraft) {
    let found = Outcome::Detected {
        draft: Box::new(draft),
        held: detecting,
        committer: Committer::Unnamed {
            git_said: "fatal: empty ident name not allowed".to_string(),
        },
    };
    let asked = state.update(Input::Outcome(detecting, found));
    assert!(asked.is_empty(), "{asked:?}");
}

/// `words` typed on the line in front, then Enter; what the state asked for.
fn typed(state: &mut State, words: &str) -> Vec<Request> {
    let mut asked = Vec::new();
    for character in words.chars() {
        asked.extend(state.update(Input::Stroke(Stroke::Insert(character))));
    }
    asked.extend(state.update(Input::Stroke(Stroke::Enter)));
    asked
}

/// The cells inside the margins of the design's wide screen, where every role names every
/// candidate.
const WIDE: usize = 118;

/// The screen's main region as Markdown writes it, laid out for the design's wide screen.
fn main_region(state: &State) -> String {
    let look = Look {
        width: Width::exact(WIDE),
        ..Markdown.look()
    };
    Markdown.draw(&state.screen(&look).main.doc)
}

/// What `operation` comes to under `ticket`, carried out by `performer`.
async fn outcome_of(performer: &Arc<Performer>, ticket: Ticket, operation: Operation) -> Outcome {
    carried_out(performer, ticket, operation).await.outcome
}

/// A git repository named `project` under `root`, never set up.
fn never_set_up(root: &Path) -> PathBuf {
    let repo = root.join("project");
    std::fs::create_dir_all(&repo).expect("a repository's directory");
    yunta_testkit::init_repo(&repo);
    repo
}

/// Where, under a test's root, no repository stands: what the Yunta repository's URL points
/// at where the pack a setup installs is out of reach.
const NOWHERE: &str = "nowhere";

/// The workspace in `repo`, its state kept under `root`, Claude Code answering, its git
/// reading `git` for the Yunta repository's URL.
fn reaching(repo: &Path, root: &Path, git: Vec<(String, String)>) -> Workspace {
    workspace_reaching(repo, root, vec![claude_answering(root)], git)
}

/// The workspace in `repo`, its state kept under `root`, Claude Code answering, the pack a
/// setup installs out of reach: no test asks the network for it.
fn out_of_reach(repo: &Path, root: &Path) -> Workspace {
    let nowhere = reference_pack::out_of_reach(&root.join(NOWHERE));
    reaching(repo, root, nowhere)
}

/// `stroke` handed to `state`, which asks for nothing.
fn pressed(state: &mut State, stroke: Stroke) {
    let asked = state.update(Input::Stroke(stroke));
    assert!(asked.is_empty(), "{stroke:?} asked {asked:?}");
}

/// Each of `strokes` handed to `state`, which asks for nothing.
fn pressing(state: &mut State, strokes: &[Stroke]) {
    for stroke in strokes {
        pressed(state, *stroke);
    }
}

/// The rows changed in place: the base typed under its row, one role's adapter changed and
/// its model typed, then `enter` on the row that writes. What is asked written is what
/// `ProjectDraft::answered` makes with the same answers, as `init -i` writes them.
#[test]
fn the_rows_changed_in_place_make_the_setup_init_writes() {
    let (mut state, detecting) = on_the_setup();
    let mut both = draft();
    both.probed[1].healthy = true;
    found_by_hand(&mut state, detecting, both.clone());
    let region = main_region(&state);
    assert!(
        region.contains("▸ **write .yunta/config.yaml**"),
        "the pointer starts on the row that writes:\n{region}"
    );
    // Up from the row that writes: the reviewer, the executor, the base, typed in place.
    pressing(&mut state, &[Stroke::Insert('k'); 3]);
    pressed(&mut state, Stroke::Enter);
    assert!(state.typing(), "the base opens a line typed in place");
    assert!(typed(&mut state, "trunk").is_empty());
    let region = main_region(&state);
    assert!(region.contains("**base** trunk"), "{region}");
    // Down to the executor: its adapter changed to Codex, a model typed for it.
    let codex = [
        Stroke::Insert('j'),
        Stroke::Enter,
        Stroke::Right,
        Stroke::Enter,
    ];
    pressing(&mut state, &codex);
    assert!(typed(&mut state, "gpt-5").is_empty());
    assert!(!state.typing(), "the role is taken");
    // Down to the row that writes, which writes it all.
    pressing(&mut state, &[Stroke::Insert('j'); 2]);
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(_, Operation::WriteSetup { held, setup, pack })] = asked.as_slice()
    else {
        panic!("`enter` on the row that writes writes the setup: {asked:?}");
    };
    assert_eq!(*held, detecting);
    assert_eq!(*pack, InstallsThePack::No, "no pack was fetched");
    let gpt = "gpt-5".parse::<ModelName>().expect("a model name");
    let answered = both.answered(
        "",
        "trunk",
        vec![RunnerAnswer {
            runner: RunnerName::from_static("executor"),
            adapter: AdapterId::from_static("codex"),
            model: Some(gpt),
        }],
    );
    assert_eq!(*setup, answered);
}

/// A row changed in place gives itself back on `esc`, and a model that names none is refused
/// where it is typed.
#[test]
fn escape_gives_a_row_back_and_a_model_that_is_no_name_is_refused() {
    let (mut state, detecting) = on_the_setup();
    found_by_hand(&mut state, detecting, draft());
    pressed(&mut state, Stroke::Insert('k'));
    pressed(&mut state, Stroke::Enter);
    pressed(&mut state, Stroke::Enter);
    for character in "not a model".chars() {
        pressed(&mut state, Stroke::Insert(character));
    }
    pressed(&mut state, Stroke::Enter);
    let region = main_region(&state);
    assert!(region.contains("model name"), "{region}");
    assert!(state.typing(), "the line stays, to be typed again");
    pressed(&mut state, Stroke::Decline);
    assert!(!state.typing());
    let region = main_region(&state);
    assert_eq!(
        role_row(&region, "reviewer").as_deref(),
        Some("claude-code"),
        "{region}"
    );
}

/// On a machine answering for two adapters, the pack a setup installs out of reach, the
/// pointer starts on the row that writes, and one `enter` writes what `init` writes unasked:
/// each role on both, in the order they were found.
#[tokio::test]
async fn enter_at_once_writes_what_init_writes_unasked() {
    let machine = |root: &Path| vec![claude_answering(root), codex_answering(root)];
    let answered = tempfile::tempdir().expect("a temporary root");
    let repo = never_set_up(answered.path());
    let clis = machine(answered.path());
    let nowhere = reference_pack::out_of_reach(&answered.path().join(NOWHERE));
    let performer = Arc::new(Performer::new(
        workspace_reaching(&repo, answered.path(), clis, nowhere),
        Channel::Tty,
    ));
    let (state, asked) = State::opened(now());
    let (mut state, surfaced) = drive(state, asked, &performer, &[Stroke::Enter]).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    assert!(
        main_region(&state).contains("for this repo's CLAUDE.md"),
        "{}",
        main_region(&state)
    );
    let unasked = tempfile::tempdir().expect("a temporary root");
    let other = never_set_up(unasked.path());
    let clis = machine(unasked.path());
    let detected = workspace_on(&other, unasked.path(), clis)
        .detect_project(false, &yunta_api::Stop::new())
        .await
        .expect("what setting it up finds");
    // What `init` writes asking nothing.
    let setup = detected.draft.answered("", "", Vec::new());
    detected.write(setup).await.expect("the setup written");
    let config = |repo: &Path| {
        std::fs::read_to_string(repo.join(".yunta/config.yaml")).expect("a config written")
    };
    assert!(
        config(&repo).contains("# detected: claude-code (2.1.0 (Claude Code))"),
        "the CLI found is the one on the workspace's PATH:\n{}",
        config(&repo)
    );
    assert!(
        config(&repo).contains(
            "runners:\n  implementer:\n    - adapter: claude-code\n    - adapter: codex\n"
        ),
        "{}",
        config(&repo)
    );
    assert_eq!(config(&repo), config(&other));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(_, Operation::ReadCatalog)] = asked.as_slice() else {
        panic!("Enter on the setup written goes on to the first run: {asked:?}");
    };
}

/// What the row of `runner` says after its label in `region`, its runs of blanks one space.
fn role_row(region: &str, runner: &str) -> Option<String> {
    let label = format!("**{runner}**");
    let row = region.lines().find(|line| line.contains(&label))?;
    let said = row.split(&label).last()?;
    Some(said.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// On a machine answering for Claude Code and then Codex, each role's row says the
/// candidates the write holds, in order: both, before anything is answered; the adapter
/// picked first, with the model typed for it, and the other after it, once a role is
/// answered.
#[test]
fn each_role_says_the_candidates_its_write_holds_in_order() {
    let (mut state, detecting) = on_the_setup();
    let mut both = draft();
    both.probed[1].healthy = true;
    found_by_hand(&mut state, detecting, both);
    let region = main_region(&state);
    for role in ["executor", "reviewer"] {
        let said = role_row(&region, role);
        assert_eq!(said.as_deref(), Some("claude-code then codex"), "{region}");
    }
    // The executor put on Codex, with a model, in place.
    pressed(&mut state, Stroke::Insert('k'));
    pressed(&mut state, Stroke::Insert('k'));
    pressed(&mut state, Stroke::Enter);
    pressed(&mut state, Stroke::Right);
    pressed(&mut state, Stroke::Enter);
    assert!(typed(&mut state, "gpt-5").is_empty());
    let region = main_region(&state);
    let executor = role_row(&region, "executor");
    assert_eq!(
        executor.as_deref(),
        Some("codex/gpt-5 then claude-code"),
        "{region}"
    );
}

#[tokio::test]
async fn a_setup_left_before_it_is_written_writes_nothing() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = never_set_up(root.path());
    let performer = Arc::new(Performer::new(
        out_of_reach(&repo, root.path()),
        Channel::Tty,
    ));
    let (mut state, asked) = State::opened(now());
    let [Request::Perform(opening, reading)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let repository = outcome_of(&performer, *opening, reading.clone()).await;
    let asked = state.update(Input::Outcome(*opening, repository));
    let [Request::Perform(detecting, Operation::DetectProject), Request::Perform(_, Operation::FetchSetupPack), Request::Follow(_)] =
        asked.as_slice()
    else {
        panic!("{asked:?}");
    };
    let found = outcome_of(&performer, *detecting, Operation::DetectProject).await;
    let Outcome::Detected { draft, .. } = &found else {
        panic!("{found:?}");
    };
    let setup = draft.answered("", "", Vec::new());
    assert!(state.update(Input::Outcome(*detecting, found)).is_empty());
    let asked = state.update(Input::Stroke(Stroke::Insert('q')));
    assert_eq!(asked, [Request::LetGo(*detecting), Request::Leave]);
    let (inputs, _received) = mpsc::unbounded_channel();
    for request in asked {
        performer.take(request, &inputs);
    }
    // What was detected was let go: writing it after fails, and writes nothing.
    let writing = Operation::WriteSetup {
        held: *detecting,
        setup,
        pack: InstallsThePack::No,
    };
    let Outcome::Failed(said) = outcome_of(&performer, *opening, writing).await else {
        panic!("a setup whose project was let go is not written");
    };
    assert_eq!(said, yunta_render::screen::SETUP_LET_GO);
    assert!(!repo.join(".yunta/config.yaml").exists());
}
