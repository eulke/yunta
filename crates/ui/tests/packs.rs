//! The packs in the application: listed from the catalog, a pack audited as
//! `yunta pack audit` prints it, one removed once a person confirmed it, the listing
//! read again; and a pack installed from the very copy its audit read, its executors
//! confirmed first, said installing while it is, or left unconfirmed with no copy behind.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::surface::{Markdown, Surface};
use tokio::sync::mpsc;
use yunta_api::{Remark, Stop};
use yunta_core::events::Channel;
use yunta_core::PackRef;
use yunta_render::page::pack::Inventory;
use yunta_render::page::Page;
use yunta_testkit_ui::driving::{
    carried_out, drive, driven_through, pressed_while, CarriedOut, Inputs, Withheld,
};
use yunta_testkit_ui::sandbox::{
    commit_of, pack_with_an_executor, with_a_pack_installed, workspace,
};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, PackOperation, PackOutcome, Performer, Request, State};

use support::on_the_field;

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

#[tokio::test]
async fn a_pack_removed_after_its_confirmation_leaves_the_listing_read_again() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = with_a_pack_installed(root.path()).await;
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7PACKS00000000000000000")]).await;
    let opened = [
        Stroke::Insert('w'),
        Stroke::Insert('p'),
        Stroke::Insert('x'),
    ];
    let (state, _) = drive(field, Vec::new(), &performer, &opened).await;
    assert!(
        state.screen(&Markdown.look()).overlay.is_some(),
        "removing a pack is confirmed first"
    );
    assert!(
        main_region(&state).contains("acme/demo"),
        "{}",
        main_region(&state)
    );
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Enter]).await;
    let shown = main_region(&state);
    assert!(
        shown.contains("pack acme/demo"),
        "the pack removed is said: {shown}"
    );
    assert!(
        shown.contains("none installed"),
        "the listing is read again: {shown}"
    );
    let listed = workspace(&repo, root.path())
        .packs()
        .list()
        .expect("the packs listed");
    assert!(listed.packs().is_empty());
}

#[tokio::test]
async fn a_pack_audited_shows_the_page_pack_audit_prints() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = with_a_pack_installed(root.path()).await;
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7PACKS00000000000000001")]).await;
    let strokes = [
        Stroke::Insert('w'),
        Stroke::Insert('p'),
        Stroke::Insert('a'),
    ];
    let (state, _) = drive(field, Vec::new(), &performer, &strokes).await;
    let pack = "acme/demo".parse::<PackRef>().expect("a pack reference");
    let remarks = |_: Remark| {};
    let audited = workspace(&repo, root.path())
        .packs()
        .audit(&pack, &remarks, &Stop::new())
        .await
        .expect("the audit `yunta pack audit acme/demo` prints");
    let printed = Inventory::Audited {
        audit: &audited.audit,
        pack: &pack,
        cases: &audited.cases,
    };
    let look = Markdown.look();
    assert_eq!(main_region(&state), Markdown.draw(&printed.document(&look)));
    let trail = state.screen(&look).trail;
    assert_eq!(trail, ["yunta", "workflows", "packs", "acme/demo"]);
}

/// A repository set up for Yunta under `root`, no pack installed.
fn set_up(root: &Path) -> PathBuf {
    let repo = root.join("project");
    std::fs::create_dir_all(repo.join(".yunta")).expect("its .yunta");
    yunta_testkit::init_repo(&repo);
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    )
    .expect("its config");
    repo
}

/// `words` typed, each character a stroke, then Enter.
fn typed(words: &str) -> Vec<Stroke> {
    let mut strokes: Vec<Stroke> = words.chars().map(Stroke::Insert).collect();
    strokes.push(Stroke::Enter);
    strokes
}

#[tokio::test]
async fn a_pack_with_an_executor_installs_the_commit_its_audit_read() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (upstream, audited) = pack_with_an_executor(root.path());
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7PACKS00000000000000002")]).await;
    let mut strokes = vec![
        Stroke::Insert('w'),
        Stroke::Insert('p'),
        Stroke::Insert('i'),
    ];
    strokes.extend(typed(upstream.to_str().expect("a path that reads")));
    // The inventory read, Enter asks to install it: its executor is confirmed first.
    strokes.push(Stroke::Enter);
    let (state, _) = drive(field, Vec::new(), &performer, &strokes).await;
    let look = Markdown.look();
    let asked = state
        .screen(&look)
        .overlay
        .expect("the executor is confirmed first");
    let asks = Markdown.draw(&asked.doc);
    assert!(
        asks.contains("runner.py"),
        "the confirmation names it: {asks}"
    );
    // The source moves on before the person confirms.
    std::fs::write(upstream.join("runner.py"), "print('runs again')\n").expect("a change");
    yunta_testkit::git(&upstream, &["commit", "-q", "-am", "v2"]);
    assert_ne!(commit_of(&upstream), audited);
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Enter]).await;
    assert!(
        main_region(&state).contains("installed"),
        "{}",
        main_region(&state)
    );
    let listed = workspace(&repo, root.path())
        .packs()
        .list()
        .expect("the packs listed");
    let yunta_render::page::pack::Listed::Locked { packs, .. } = listed else {
        panic!("the pack is installed: {listed:?}");
    };
    let [installed] = packs.as_slice() else {
        panic!("{packs:?}");
    };
    assert_eq!(installed.locked.commit.to_string(), audited);
}

/// A pack whose executors are confirmed shows that it is installing while it is, `esc`
/// letting nothing go meanwhile, and what installing came to lands on the packs' page.
#[tokio::test]
async fn a_confirmed_install_shows_it_installs_and_its_answer_lands_on_the_packs() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (upstream, _) = pack_with_an_executor(root.path());
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7PACKS00000000000000004")]).await;
    let mut strokes = vec![
        Stroke::Insert('w'),
        Stroke::Insert('p'),
        Stroke::Insert('i'),
    ];
    strokes.extend(typed(upstream.to_str().expect("a path that reads")));
    // The inventory's Enter asks its executor confirmed, Enter confirms it, and `esc` comes
    // while the install is under way.
    strokes.extend([Stroke::Enter, Stroke::Enter, Stroke::Decline]);
    let installs =
        |work: &Operation| matches!(work, Operation::Pack(PackOperation::Install { .. }));
    let mut inputs = Inputs::new();
    let pressed = pressed_while(
        &mut inputs,
        field,
        Vec::new(),
        &performer,
        installs,
        &strokes,
    );
    let Withheld { driven, withheld } = pressed.await;
    let shown = main_region(&driven.state);
    assert!(
        shown.contains("installing ") && !shown.contains("runner.py"),
        "{shown}"
    );
    let [(_, Operation::Pack(PackOperation::Install { .. }))] = withheld.as_slice() else {
        panic!("the pack is installed once: {withheld:?}");
    };
    let installing = withheld
        .into_iter()
        .map(|(ticket, work)| Request::Perform(ticket, work))
        .collect();
    let done = driven_through(&mut inputs, driven.state, installing, &performer, &[]).await;
    let shown = main_region(&done.state);
    assert!(shown.contains("installed"), "{shown}");
}

#[tokio::test]
async fn a_pack_left_unconfirmed_leaves_no_temporary_checkout() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (upstream, _) = pack_with_an_executor(root.path());
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7PACKS00000000000000003")]).await;
    let opened = [
        Stroke::Insert('w'),
        Stroke::Insert('p'),
        Stroke::Insert('i'),
    ];
    let (mut state, _) = drive(field, Vec::new(), &performer, &opened).await;
    let mut asked = Vec::new();
    for stroke in typed(upstream.to_str().expect("a path that reads")) {
        asked = state.update(Input::Stroke(stroke));
    }
    let [Request::Perform(fetching, fetch @ Operation::Pack(PackOperation::Fetch { .. }))] =
        asked.as_slice()
    else {
        panic!("the pack is fetched: {asked:?}");
    };
    let CarriedOut { outcome, .. } = carried_out(&performer, *fetching, fetch.clone()).await;
    let fetched = (*fetching, outcome);
    let Outcome::Pack(PackOutcome::Fetched(copy)) = &fetched.1 else {
        panic!("{:?}", fetched.1);
    };
    let (checkout, held) = (copy.checkout.clone(), copy.held);
    assert!(
        checkout.exists(),
        "the copy its audit reads stands while it is held"
    );
    state.update(Input::Outcome(fetched.0, fetched.1));
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert_eq!(asked, [Request::LetGo(held)]);
    let (inputs, _received) = mpsc::unbounded_channel();
    for request in asked {
        performer.take(request, &inputs);
    }
    assert!(!checkout.exists(), "a copy let go leaves nothing behind");
}

#[tokio::test]
async fn leaving_the_packs_reads_the_workflows_again() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = with_a_pack_installed(root.path()).await;
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7PACKS00000000000000003")]).await;
    let to_the_packs = [Stroke::Insert('w'), Stroke::Insert('p')];
    let (mut state, _) = drive(field, Vec::new(), &performer, &to_the_packs).await;
    // What a pack installed, updated or removed changes the workflows the catalog offers.
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(
        asked
            .iter()
            .any(|request| matches!(request, Request::Perform(_, Operation::ReadCatalog))),
        "{asked:?}"
    );
}

/// The packs' listing refused: what refused it is said, and the packs are no longer said to be
/// read, as nothing reads them.
#[tokio::test]
async fn a_refused_listing_of_the_packs_is_said_and_no_longer_said_to_read() {
    let (mut state, _) =
        on_the_field(&[&yunta_core::RunId::from("01M7PACKS00000000000000004")]).await;
    state.update(Input::Stroke(Stroke::Insert('w')));
    let asked = state.update(Input::Stroke(Stroke::Insert('p')));
    let listing = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::Pack(PackOperation::List)) => Some(*ticket),
        _ => None,
    });
    let listing = listing.unwrap_or_else(|| panic!("the packs are listed: {asked:?}"));
    let refused = yunta_render::refusal::Refused::NoRunHere;
    let said = refused.to_string();
    state.update(Input::Outcome(listing, Outcome::Refused(refused)));
    let shown = main_region(&state);
    assert!(shown.contains(&said), "{shown}");
    assert!(
        !shown.contains(yunta_render::screen::READING_THE_PACKS),
        "{shown}"
    );
    assert_eq!(state.shown_under_way(), None);
}
