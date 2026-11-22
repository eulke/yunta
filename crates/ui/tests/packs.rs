//! The packs in the application: listed from the catalog, a pack audited as
//! `yunta pack audit` prints it, one removed once a person confirmed it, the listing
//! read again; and a pack installed from the very copy its audit read, its executors
//! confirmed first, or left unconfirmed with no copy behind. `g` turns the listing to
//! the user's packs, and what is done there is done to them.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::surface::{Markdown, Surface};
use tokio::sync::mpsc;
use yunta_api::{Remark, Stop};
use yunta_core::{PackRef, PackScope};
use yunta_render::page::pack::Inventory;
use yunta_render::page::Page;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{
    commit_of, pack_with_an_executor, with_a_pack_installed, workspace,
};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, PackOperation, PackOutcome, Performer, Request, State};

use support::{on_the_field, stopped};

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

#[tokio::test]
async fn a_pack_removed_after_its_confirmation_leaves_the_listing_read_again() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = with_a_pack_installed(root.path()).await;
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7PACKS00000000000000000",
    ))]);
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
        .packs(PackScope::Project)
        .and_then(|packs| packs.list())
        .expect("the packs listed");
    assert!(listed.packs().is_empty());
}

#[tokio::test]
async fn a_pack_audited_shows_the_page_pack_audit_prints() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = with_a_pack_installed(root.path()).await;
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7PACKS00000000000000001",
    ))]);
    let strokes = [
        Stroke::Insert('w'),
        Stroke::Insert('p'),
        Stroke::Insert('a'),
    ];
    let (state, _) = drive(field, Vec::new(), &performer, &strokes).await;
    let pack = "acme/demo".parse::<PackRef>().expect("a pack reference");
    let remarks = |_: Remark| {};
    let working = workspace(&repo, root.path());
    let packs = working
        .packs(PackScope::Project)
        .expect("this project's packs");
    let audited = packs
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
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7PACKS00000000000000002",
    ))]);
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
        .packs(PackScope::Project)
        .and_then(|packs| packs.list())
        .expect("the packs listed");
    let yunta_render::page::pack::Listed::Locked { packs, .. } = listed else {
        panic!("the pack is installed: {listed:?}");
    };
    let [installed] = packs.as_slice() else {
        panic!("{packs:?}");
    };
    assert_eq!(installed.locked.commit.to_string(), audited);
}

/// `g` on the listing shows the user's packs, which every project reads, and a pack
/// installed while they are shown is installed among them: under the state root, with
/// nothing written to the project's packs, and listed there once it is in.
#[tokio::test]
async fn g_lists_the_users_packs_and_an_install_made_there_lands_among_them() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (upstream, _) = pack_with_an_executor(root.path());
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7PACKS00000000000000004",
    ))]);
    let to_the_users = [
        Stroke::Insert('w'),
        Stroke::Insert('p'),
        Stroke::Insert('g'),
    ];
    let (state, _) = drive(field, Vec::new(), &performer, &to_the_users).await;
    let shown = main_region(&state);
    assert!(
        shown.contains("the user's packs") && shown.contains("none installed"),
        "{shown}"
    );
    let mut strokes = vec![Stroke::Insert('i')];
    strokes.extend(typed(upstream.to_str().expect("a path that reads")));
    // The inventory read, Enter asks to install it, and its executor is confirmed.
    strokes.extend([Stroke::Enter, Stroke::Enter]);
    let (state, _) = drive(state, Vec::new(), &performer, &strokes).await;
    assert!(
        main_region(&state).contains("installed"),
        "{}",
        main_region(&state)
    );
    assert!(root.path().join("packs/acme/tools/pack.yaml").is_file());
    assert!(!repo.join(".yunta/packs").exists());
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Enter]).await;
    let shown = main_region(&state);
    assert!(
        shown.contains("the user's packs") && shown.contains("acme/tools"),
        "the user's packs are read again: {shown}"
    );
}

#[tokio::test]
async fn a_pack_left_unconfirmed_leaves_no_temporary_checkout() {
    let root = tempfile::tempdir().expect("a temporary root");
    let (upstream, _) = pack_with_an_executor(root.path());
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7PACKS00000000000000003",
    ))]);
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
    let (inputs, mut received) = mpsc::unbounded_channel();
    performer.perform(*fetching, fetch.clone(), inputs).await;
    let fetched = loop {
        match received.recv().await.expect("the fetch hands back") {
            Input::Outcome(ticket, outcome) => break (ticket, outcome),
            _ => continue,
        }
    };
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
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7PACKS00000000000000003",
    ))]);
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
