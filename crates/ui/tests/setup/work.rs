//! See [`super`]. The setup's work, as the performer carries it out: detecting the repository
//! says who git commits as; fetching the pack a setup installs, work of its own, comes to its
//! audit and the roles it proposes, or says why it is out of reach in git's own words, or that
//! it is installed already; and the setup written with the pack or without it leaves what a
//! twin repository leaves once `init` writes it, the pack fetched and installed first or not.
//! What writing comes to is the setup's wherever it stands as it lands, a page opened over it
//! meanwhile among them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::glyphs::Glyphs;
use surco::look::Look;
use yunta_api::{Committer, SetupPack, Workspace};
use yunta_core::events::Channel;
use yunta_core::{PackRef, RunnerName};
use yunta_render::screen;
use yunta_testkit::reference_pack::ReferencePack;
use yunta_testkit_ui::driving::{driven_through, pressed_while, Inputs, Withheld};
use yunta_testkit_ui::sandbox::now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{
    Input, InstallsThePack, Operation, Outcome, Performer, Request, SetupPackFetch, Stage, State,
    Ticket,
};

use super::{
    main_region, never_set_up, on_the_setup_fetching, out_of_reach, outcome_of, reaching, NOWHERE,
};

/// `repo`'s git committing as Marta Ruiz.
fn committing_as_marta(repo: &Path) {
    yunta_testkit::git(repo, &["config", "user.name", "Marta Ruiz"]);
    yunta_testkit::git(repo, &["config", "user.email", "marta.ruiz@example.com"]);
}

/// What the performer comes to setting the repository up, under the tickets the state asks
/// each under as the setup opens: what was found, what fetching the pack came to, and the
/// ticket the project is held under, the pack with it.
async fn found_and_fetched(performer: &Arc<Performer>) -> (Outcome, Outcome, Ticket) {
    let (_, detecting, fetching) = on_the_setup_fetching();
    let found = outcome_of(performer, detecting, Operation::DetectProject).await;
    let fetched = outcome_of(performer, fetching, Operation::FetchSetupPack).await;
    (found, fetched, detecting)
}

/// The work the setup's opening asks for, the detection and the pack's fetch, each with the
/// ticket it goes under: panics where the opening does not ask both.
fn opening_work(asked: &[Request]) -> [(Ticket, Operation); 2] {
    let asked_for = |which: fn(&Operation) -> bool| {
        asked.iter().find_map(|request| match request {
            Request::Perform(ticket, work) if which(work) => Some((*ticket, work.clone())),
            _ => None,
        })
    };
    let detect = asked_for(|work| matches!(work, Operation::DetectProject));
    let fetch = asked_for(|work| matches!(work, Operation::FetchSetupPack));
    let (Some(detect), Some(fetch)) = (detect, fetch) else {
        panic!(
            "the setup asks what the repository runs and its pack fetched as it opens: {asked:?}"
        );
    };
    [detect, fetch]
}

/// The pack a setup installs, fetched as the setup opens beside the detection, landing before
/// the project or after it: either order comes to the same rows, and the write, once the
/// pack's audit is read and its install confirmed, installs the pack held with the project.
#[tokio::test]
async fn the_pack_landing_before_or_after_the_project_comes_to_the_same_rows_and_setup() {
    let mut drawn = Vec::new();
    for fetch_first in [true, false] {
        let root = tempfile::tempdir().expect("a temporary root");
        let reference = ReferencePack::made(&root.path().join("upstream/yunta"));
        let repo = never_set_up(root.path());
        committing_as_marta(&repo);
        let workspace = reaching(&repo, root.path(), reference.reached());
        let performer = Arc::new(Performer::new(workspace, Channel::Tty));
        let (mut state, asked) = State::opened(now());
        let [Request::Perform(opening, reading)] = asked.as_slice() else {
            panic!("{asked:?}");
        };
        let read = outcome_of(&performer, *opening, reading.clone()).await;
        let [detect, fetch] = opening_work(&state.update(Input::Outcome(*opening, read)));
        let order = match fetch_first {
            true => [fetch, detect],
            false => [detect, fetch],
        };
        for (ticket, work) in order {
            let done = outcome_of(&performer, ticket, work).await;
            let asked = state.update(Input::Outcome(ticket, done));
            assert!(asked.is_empty(), "{asked:?}");
        }
        drawn.push(main_region(&state));
        for stroke in [Stroke::Enter, Stroke::Enter] {
            let asked = state.update(Input::Stroke(stroke));
            assert!(
                asked.is_empty(),
                "the audit, then its confirmation: {asked:?}"
            );
        }
        let asked = state.update(Input::Stroke(Stroke::Enter));
        let [Request::Perform(writing, write)] = asked.as_slice() else {
            panic!("{asked:?}");
        };
        let written = outcome_of(&performer, *writing, write.clone()).await;
        let installed = matches!(
            &written,
            Outcome::Initialized {
                installed: Some(_),
                ..
            }
        );
        assert!(installed, "the pack held with the project: {written:?}");
    }
    assert!(drawn[0].contains("to install as a pack"), "{}", drawn[0]);
    assert_eq!(drawn[0], drawn[1]);
}

/// Every file under `dir`, by its path under it, with its bytes: what a twin's must equal.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut walked = vec![dir.to_path_buf()];
    while let Some(at) = walked.pop() {
        for entry in std::fs::read_dir(&at).expect("a directory, read") {
            let path = entry.expect("an entry").path();
            match path.is_dir() {
                true => walked.push(path),
                false => {
                    let under = path
                        .strip_prefix(dir)
                        .expect("under the tree")
                        .to_path_buf();
                    files.insert(under, std::fs::read(&path).expect("a file, read"));
                }
            }
        }
    }
    files
}

/// What a setup leaves in `repo`: its config, its lock where one is written, and the tree
/// of `yunta/fragua` vendored there.
fn left_by_the_setup(repo: &Path) -> (String, Option<String>, BTreeMap<PathBuf, Vec<u8>>) {
    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).expect("a config");
    let lock = std::fs::read_to_string(repo.join(".yunta/yunta.lock")).ok();
    let fragua = repo.join(".yunta/packs/yunta/fragua");
    let vendored = match fragua.exists() {
        true => tree(&fragua),
        false => BTreeMap::new(),
    };
    (config, lock, vendored)
}

/// What a twin of a repository leaves once `init` writes it with every answer left, after
/// the pack a setup installs is fetched and installed there where `with_the_pack` says.
async fn twin_set_up(workspace: &Workspace, with_the_pack: bool) {
    let stop = yunta_api::Stop::new();
    if with_the_pack {
        let quiet = |_: yunta_api::Remark| {};
        let source = SetupPack::of_this_binary();
        let fetched = workspace.packs().fetch(source.source(), true, &stop).await;
        let fetched = fetched.expect("the setup's pack, fetched");
        fetched
            .install(&quiet, &stop)
            .await
            .expect("the setup's pack, installed");
    }
    let detected = workspace.detect_project(false, &stop).await;
    let detected = detected.expect("what setting it up finds");
    let setup = detected.draft.answered("", "", Vec::new());
    detected.write(setup).await.expect("the setup written");
}

/// Detecting a repository whose git points the Yunta repository's URL at the local fragua
/// says who git commits as; the pack's fetch, a piece of work of its own beside it, comes to
/// fragua's audit and the roles the project proposes once fragua is installed.
#[tokio::test]
async fn the_setup_finds_who_commits_then_fetches_fragua_with_its_audit_and_roles() {
    let root = tempfile::tempdir().expect("a temporary root");
    let reference = ReferencePack::made(&root.path().join("upstream/yunta"));
    let repo = never_set_up(root.path());
    committing_as_marta(&repo);
    let workspace = reaching(&repo, root.path(), reference.reached());
    let performer = Arc::new(Performer::new(workspace, Channel::Tty));
    let (found, fetched, _) = found_and_fetched(&performer).await;
    let Outcome::Detected { committer, .. } = &found else {
        panic!("{found:?}");
    };
    assert!(
        matches!(committer, Committer::Named(who) if who.name == "Marta Ruiz"),
        "{committer:?}"
    );
    let Outcome::SetupPack {
        fetched: offered, ..
    } = &fetched
    else {
        panic!("{fetched:?}");
    };
    let SetupPackFetch::Fetched {
        pack,
        audit,
        proposed,
        ..
    } = &**offered
    else {
        panic!("fragua is in reach: {offered:?}");
    };
    let fragua: PackRef = "yunta/fragua".parse().expect("a pack");
    assert_eq!(pack, &fragua);
    assert_eq!(audit.manifest.reference(), fragua);
    let roles: Vec<&str> = proposed.roles().iter().map(RunnerName::as_str).collect();
    assert_eq!(
        roles,
        [
            "planner",
            "executor",
            "reviewer",
            "reviewer-alt",
            "mechanical"
        ]
    );
    assert!(
        !repo.join(".yunta").exists(),
        "nothing is written until the setup is"
    );
}

/// Written with the pack it fetched, a setup leaves the config, the lock and fragua's tree a
/// twin leaves once fragua is fetched and installed and `init` writes it with the same
/// answers; written without it, the twin's with no pack.
#[tokio::test]
async fn the_setup_written_with_and_without_its_pack_leaves_what_its_twin_does() {
    for installs in [InstallsThePack::Yes, InstallsThePack::No] {
        let root = tempfile::tempdir().expect("a temporary root");
        let reference = ReferencePack::made(&root.path().join("upstream/yunta"));
        let set_up = root.path().join("set-up");
        let repo = never_set_up(&set_up);
        let workspace = reaching(&repo, root.path(), reference.reached());
        let performer = Arc::new(Performer::new(workspace, Channel::Tty));
        let (found, fetched, held) = found_and_fetched(&performer).await;
        let Outcome::Detected { draft, .. } = found else {
            panic!("{found:?}");
        };
        let Outcome::SetupPack {
            fetched: offered, ..
        } = &fetched
        else {
            panic!("{fetched:?}");
        };
        let SetupPackFetch::Fetched { proposed, .. } = &**offered else {
            panic!("fragua is in reach: {offered:?}");
        };
        let mut answered = *draft;
        if installs == InstallsThePack::Yes {
            answered.proposed = proposed.clone();
        }
        let writing = Operation::WriteSetup {
            held,
            setup: answered.answered("", "", Vec::new()),
            pack: installs,
        };
        let written = outcome_of(&performer, held, writing).await;
        let Outcome::Initialized { installed, .. } = &written else {
            panic!("{written:?}");
        };
        assert_eq!(installed.is_some(), installs == InstallsThePack::Yes);
        let twin_root = root.path().join("twin");
        let twin = never_set_up(&twin_root);
        let twin_workspace = reaching(&twin, &twin_root, reference.reached());
        twin_set_up(&twin_workspace, installs == InstallsThePack::Yes).await;
        let (left, twins) = (left_by_the_setup(&repo), left_by_the_setup(&twin));
        assert_eq!(left.0, twins.0, "{installs:?}: the config");
        assert_eq!(left.1, twins.1, "{installs:?}: the lock");
        assert!(left.2 == twins.2, "{installs:?}: fragua's tree");
        assert_eq!(left.1.is_some(), installs == InstallsThePack::Yes);
        assert_eq!(left.2.is_empty(), installs == InstallsThePack::No);
    }
}

/// With the Yunta repository's URL pointing at nothing, the pack's fetch says why it cannot
/// be reached in git's own words, not the command and the directory it cloned into; and the
/// setup is written alone.
#[tokio::test]
async fn a_pack_out_of_reach_says_gits_words_and_the_setup_is_written_alone() {
    let root = tempfile::tempdir().expect("a temporary root");
    std::fs::create_dir_all(root.path().join(NOWHERE)).expect("a directory no repository is");
    let repo = never_set_up(root.path());
    let performer = Arc::new(Performer::new(
        out_of_reach(&repo, root.path()),
        Channel::Tty,
    ));
    let (found, fetched, held) = found_and_fetched(&performer).await;
    let Outcome::SetupPack {
        fetched: offered, ..
    } = &fetched
    else {
        panic!("{fetched:?}");
    };
    let SetupPackFetch::OutOfReach { why, .. } = &**offered else {
        panic!("fragua is out of reach: {offered:?}");
    };
    let first = why.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("fatal: '") && first.ends_with("does not appear to be a git repository"),
        "{why}"
    );
    let Outcome::Detected { draft, .. } = found else {
        panic!("{found:?}");
    };
    let writing = Operation::WriteSetup {
        held,
        setup: draft.answered("", "", Vec::new()),
        pack: InstallsThePack::Yes,
    };
    let written = outcome_of(&performer, held, writing).await;
    let Outcome::Initialized { installed, .. } = &written else {
        panic!("{written:?}");
    };
    assert!(installed.is_none(), "{written:?}");
    let (_, lock, vendored) = left_by_the_setup(&repo);
    assert_eq!(lock, None);
    assert!(vendored.is_empty());
}

/// A repository that holds fragua vendored already, never set up, its source out of reach:
/// the setup reads that the pack is installed with no network asked, not that it is out of
/// reach, and the project proposes the installed pack's roles.
#[tokio::test]
async fn a_pack_installed_already_is_said_installed_with_no_network_asked() {
    let root = tempfile::tempdir().expect("a temporary root");
    let reference = ReferencePack::made(&root.path().join("upstream/yunta"));
    let repo = never_set_up(root.path());
    let workspace = reaching(&repo, root.path(), reference.reached());
    let stop = yunta_api::Stop::new();
    let quiet = |_: yunta_api::Remark| {};
    let source = SetupPack::of_this_binary();
    let fetched = workspace.packs().fetch(source.source(), true, &stop).await;
    let fetched = fetched.expect("the setup's pack, fetched");
    fetched.install(&quiet, &stop).await.expect("installed");
    let performer = Arc::new(Performer::new(
        out_of_reach(&repo, root.path()),
        Channel::Tty,
    ));
    let (found, fetched, _) = found_and_fetched(&performer).await;
    let Outcome::SetupPack {
        fetched: offered, ..
    } = &fetched
    else {
        panic!("{fetched:?}");
    };
    assert!(
        matches!(&**offered, SetupPackFetch::Installed { .. }),
        "{offered:?}"
    );
    let Outcome::Detected { draft, .. } = found else {
        panic!("{found:?}");
    };
    let roles: Vec<&str> = draft
        .proposed
        .roles()
        .iter()
        .map(RunnerName::as_str)
        .collect();
    assert_eq!(
        roles,
        [
            "planner",
            "executor",
            "reviewer",
            "reviewer-alt",
            "mechanical"
        ]
    );
}

/// A setup opened over while it writes comes back written: what writing came to is the
/// setup's, wherever it stands as it lands, and the page opened over it takes nothing of it.
/// The workflows opened from the palette while the write is under way, the write lands, and
/// `esc` comes back to what was written, its keys going on from there.
#[tokio::test]
async fn a_setup_opened_over_while_it_writes_comes_back_written() {
    let root = tempfile::tempdir().expect("a temporary root");
    std::fs::create_dir_all(root.path().join(NOWHERE)).expect("a directory no repository is");
    let repo = never_set_up(root.path());
    committing_as_marta(&repo);
    let performer = Arc::new(Performer::new(
        out_of_reach(&repo, root.path()),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let mut inputs = Inputs::new();
    let writes = |work: &Operation| matches!(work, Operation::WriteSetup { .. });
    let over = [Stroke::Enter, Stroke::Insert('/'), Stroke::Enter];
    let pressed = pressed_while(&mut inputs, opened, asked, &performer, writes, &over);
    let Withheld { driven, withheld } = pressed.await;
    assert!(
        matches!(driven.state.stage(), Some(Stage::Catalog(_))),
        "the workflows stand over the setup while it writes"
    );
    let [(_, Operation::WriteSetup { .. })] = withheld.as_slice() else {
        panic!("the setup is written once: {withheld:?}");
    };
    let written = withheld
        .into_iter()
        .map(|(ticket, work)| Request::Perform(ticket, work))
        .collect();
    let state = driven.state;
    let back = driven_through(&mut inputs, state, written, &performer, &[Stroke::Decline]).await;
    let shown = main_region(&back.state);
    assert!(!shown.contains(screen::WRITING_THE_SETUP), "{shown}");
    let legend = back.state.screen(&Look::plain()).legend;
    let glyphs = Glyphs::Unicode;
    assert_eq!(
        super::support::spelled(&legend, glyphs),
        super::support::spelled(&screen::written_legend(), glyphs),
        "{shown}"
    );
}

/// A setup whose detection fails lets go the pack fetched for it, whichever of the two lands
/// first: nothing holds the copy the fetch made once nothing can be written with it.
#[tokio::test]
async fn a_setup_whose_detection_fails_lets_go_the_pack_fetched_for_it() {
    for fetched_first in [true, false] {
        let root = tempfile::tempdir().expect("a temporary root");
        let reference = ReferencePack::made(&root.path().join("upstream/yunta"));
        let repo = never_set_up(root.path());
        let workspace = reaching(&repo, root.path(), reference.reached());
        let performer = Arc::new(Performer::new(workspace, Channel::Tty));
        let (mut state, detecting, fetching) = on_the_setup_fetching();
        let fetched = outcome_of(&performer, fetching, Operation::FetchSetupPack).await;
        let Outcome::SetupPack { fetched: pack, .. } = &fetched else {
            panic!("{fetched:?}");
        };
        assert!(
            matches!(**pack, SetupPackFetch::Fetched { .. }),
            "fragua is in reach: {pack:?}"
        );
        let unfound = Outcome::Failed("git: not a repository".to_string());
        let landed = match fetched_first {
            true => [(fetching, fetched), (detecting, unfound)],
            false => [(detecting, unfound), (fetching, fetched)],
        };
        let mut asked = Vec::new();
        for (ticket, outcome) in landed {
            asked.extend(state.update(Input::Outcome(ticket, outcome)));
        }
        assert!(
            asked.contains(&Request::LetGo(fetching)),
            "fetched first: {fetched_first}, {asked:?}"
        );
    }
}
