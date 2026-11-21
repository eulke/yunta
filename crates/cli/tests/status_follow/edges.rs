//! A follower at the edges of a run: a promotion it follows onto the run that
//! succeeds it, a parent with a child open, and followers that change nothing.

use super::*;

use std::process::Output;

use yunta_testkit::workflows::PROMOTABLE;

/// The block a drawing ends on, from the last headline that names `named`.
fn block_from(drawn: &str, named: &str) -> String {
    let plain = without_sequences(drawn);
    let at = plain
        .rfind(named)
        .unwrap_or_else(|| panic!("no `{named}` in:\n{plain}"));
    unwrapped(&plain[at..])
}

/// A run promoted while it is followed is followed onto the run that succeeds
/// it, to the end `yunta run` reaches with the same chain: the successor's
/// block and code.
///
/// Reached by: `crates/cli/src/commands/status/follow.rs`, `crates/api/src/follow/following.rs`,
/// `crates/api/src/follow/successor.rs`
#[test]
fn a_promotion_is_followed_onto_its_successor() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", PROMOTABLE)
        .committed();
    let mut attached = yunta_on_terminal!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml", "--mode", "quick"]
    );
    attached.wait_on_screen("▸ retry", "the exhausted reroute never asked");
    let first = std::fs::read_dir(runs_root(&checkout.home))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .next()
        .unwrap();
    let mut follower = yunta_on_terminal!(
        &checkout.repo,
        &checkout.home,
        &["status", &first, "--follow"]
    );
    follower.wait_for(
        "asks at its terminal",
        "the follower never saw the question",
    );

    attached.choose(&["abort"], "promote");
    attached.wait_on_screen("▸ retry", "the successor never asked");
    attached.choose(&[], "abort");
    let driven = attached.ended();
    let followed = follower.ended();
    assert_eq!(
        block_from(&followed, "promotable (full) ·"),
        block_from(&driven, "promotable (full) ·")
    );
    // `abort` on an exhausted reroute fails the node, so the chain stops with 1.
    assert_eq!(attached.exit_code(), Some(1));
    assert_eq!(follower.exit_code(), attached.exit_code());
}

/// A run that stands for one an hour old: the promoted run copied under its id, its
/// log stamped an hour earlier and its directory beside it, which no run succeeds.
const AN_HOUR_AGO: &str = "01M7F0110WEDAN0LDPR0M0T10N";

/// The run `run` of `home` copied as [`AN_HOUR_AGO`], every event an hour earlier.
fn an_hour_before(home: &Path, run: &str) -> String {
    let storage = yunta_storage::Storage::open(&home.join("yunta.db")).unwrap();
    let copy = yunta_core::RunId::from(AN_HOUR_AGO);
    for event in storage
        .events_for_run(&yunta_core::RunId::from(run))
        .unwrap()
    {
        let payload = event.payload().expect("an engine's own kinds").clone();
        let draft = yunta_core::events::EventDraft::new(copy.clone(), event.node_id, payload);
        let earlier = event.timestamp - chrono::TimeDelta::hours(1);
        storage.append_at(&draft.unwrap(), earlier).unwrap();
    }
    let runs = runs_root(home);
    let copied = std::process::Command::new("cp")
        .arg("-R")
        .args([runs.join(run), runs.join(AN_HOUR_AGO)])
        .status();
    assert!(copied.unwrap().success(), "the run's directory copied");
    AN_HOUR_AGO.to_string()
}

/// The lines of a page below its headline, which counts how long ago the run stood.
fn below_the_headline(page: &Output) -> Vec<String> {
    stdout(page).lines().skip(1).map(str::to_string).collect()
}

/// A run promoted before it is followed goes on with its successor already in the log,
/// whose page the follower ends on as a follower of the successor does; one promoted an
/// hour before, with none, moved nowhere while it was followed: its page at once, and
/// the code its stop gives.
///
/// Reached by: `crates/cli/src/commands/status/follow.rs`, `crates/api/src/follow/successor.rs`,
/// `crates/api/src/event_log.rs`
#[test]
fn a_run_promoted_before_it_is_followed_ends_on_its_successor_or_its_own_page() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", PROMOTABLE)
        .committed();
    let (repo, home) = (&checkout.repo, &checkout.home);
    let mut attached = yunta_on_terminal!(repo, home, &["run", "wf.yaml", "--mode", "quick"]);
    attached.wait_on_screen("▸ retry", "the exhausted reroute never asked");
    let first = yunta_testkit::run_ids(home).remove(0);
    attached.choose(&["abort"], "promote");
    attached.wait_on_screen("▸ retry", "the successor never asked");
    attached.choose(&[], "abort");
    attached.ended();
    let runs = yunta_testkit::run_ids(home);
    let successor = runs.iter().find(|run| **run != first).expect("a successor");

    let long_ago = an_hour_before(home, &first);
    let page = yunta_in!(repo, home, &["status", &long_ago]);
    let followed = yunta_in!(repo, home, &["status", &long_ago, "--follow"]);
    assert_eq!(below_the_headline(&followed), below_the_headline(&page));
    // A promotion is no success: its stop gives 1.
    assert_eq!(followed.status.code(), Some(1), "{}", stdout(&followed));

    let onto = yunta_in!(repo, home, &["status", &first, "--follow"]);
    let its_own = yunta_in!(repo, home, &["status", successor, "--follow"]);
    assert_eq!(below_the_headline(&onto), below_the_headline(&its_own));
    assert_eq!(onto.status.code(), its_own.status.code());
}

/// A child that holds until `go` exists under `marks`, or `marks` is gone.
fn child(marks: &Path) -> String {
    let marks = marks.display();
    format!(
        "name: child\nnodes:\n  - {{ id: work, kind: bash, run: \"until [ -f {marks}/go ] || [ ! -d {marks} ]; do sleep 0.05; done\" }}\n"
    )
}

/// A parent's region shows its open child under the node that bore it, as
/// `yunta run` shows it: what the parent's own log says of it.
#[test]
fn a_parent_is_followed_with_its_children_under_their_nodes() {
    let root = tempfile::tempdir().unwrap();
    let marks = root.path().join("marks");
    std::fs::create_dir_all(&marks).unwrap();
    let checkout = Checkout::under(root.path())
        .config("defaults:\n  isolation: none\n")
        .file(".yunta/workflows/child.yaml", &child(&marks))
        .workflow(
            "wf",
            "name: parent\nnodes:\n  - { id: compose, kind: workflow, use: child }\n",
        )
        .committed();
    let detached = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml", "--detach"]
    );
    assert!(detached.status.success(), "{}", stdout(&detached));
    let parent = run_id_from(&detached);
    let parent = yunta_testkit::full_run_id(&checkout.home, &parent);

    let mut follower = yunta_on_terminal!(
        &checkout.repo,
        &checkout.home,
        &["status", &parent, "--follow"]
    );
    follower.wait_for("child run ", "the follower never drew the open child");
    std::fs::write(marks.join("go"), "go").unwrap();
    let followed = follower.ended();
    assert_eq!(follower.exit_code(), Some(0), "{followed}");
}

/// A moving run whose manifest carries a key a newer binary wrote: its follower says so
/// once, in the words `yunta status` says it with, while the run moves on.
#[test]
fn a_manifests_unread_keys_are_said_once_by_a_follower() {
    let root = tempfile::tempdir().unwrap();
    let marks = root.path().join("marks");
    let checkout = holding(root.path(), &marks);
    let detached = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml", "--detach"]
    );
    assert!(detached.status.success(), "{}", stdout(&detached));
    let id = the_held_run(&checkout.home, &marks);
    let manifest = runs_root(&checkout.home).join(&id).join("manifest.yaml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("something_later: 7\n");
    std::fs::write(&manifest, text).unwrap();
    let status = yunta_in!(&checkout.repo, &checkout.home, &["status", &id]);
    let said = yunta_testkit::stderr(&status);
    let unread = said
        .lines()
        .find(|line| line.contains("something_later"))
        .unwrap_or_else(|| panic!("`yunta status` says the key: {said}"));

    let (mut follower, _, err) = following_off_a_terminal(&checkout, &id, root.path());
    std::fs::write(marks.join("go"), "go").unwrap();
    assert_eq!(follower.wait().unwrap().code(), Some(0));
    let followed = std::fs::read_to_string(err).unwrap();
    assert_eq!(followed.matches(unread).count(), 1, "{followed}");
}

/// What a run's registry and lock hold, byte for byte.
fn held_files(home: &Path, id: &str) -> (Vec<u8>, Vec<u8>) {
    let scratch = runs_root(home).join(id).join("scratch");
    (
        std::fs::read(scratch.join("engine.json")).unwrap(),
        std::fs::read(scratch.join("engine.lock")).unwrap(),
    )
}

/// Waits until the registry of the run `id` names the process group of the
/// node it holds. The engine writes it there once the node is spawned, which can
/// land after the node's own first command, so a reading taken before it would
/// see the engine change the registry, not a follower.
fn registered_the_held_node(home: &Path, id: &str) {
    let registry = runs_root(home).join(id).join("scratch").join("engine.json");
    wait_until(
        || {
            std::fs::read(&registry)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|engine| {
                    engine["process_groups"]
                        .as_array()
                        .map(|groups| !groups.is_empty())
                })
                .unwrap_or(false)
        },
        || "the engine never registered the held node's process group".to_string(),
    );
}

/// The kinds of the run `id`'s log, in order.
fn kinds(home: &Path, id: &str) -> Vec<String> {
    yunta_storage::Storage::open(&home.join("yunta.db"))
        .unwrap()
        .events_for_run(&yunta_core::RunId::from(id))
        .unwrap()
        .iter()
        .map(|event| event.body.kind_name().to_string())
        .collect()
}

/// A follower only reads. Two of them on one run leave its registry and its
/// lock as they were, and its log holds what a run nobody followed holds.
///
/// Reached by: `crates/cli/src/commands/status/follow.rs`, `crates/api/src/follow/following.rs`
#[test]
fn two_followers_change_nothing() {
    let root = tempfile::tempdir().unwrap();
    let marks = root.path().join("marks");
    let checkout = holding(root.path(), &marks);
    let detached = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml", "--detach"]
    );
    assert!(detached.status.success(), "{}", stdout(&detached));
    let id = the_held_run(&checkout.home, &marks);
    registered_the_held_node(&checkout.home, &id);
    let before = held_files(&checkout.home, &id);

    let one = root.path().join("one");
    let two = root.path().join("two");
    std::fs::create_dir_all(&one).unwrap();
    std::fs::create_dir_all(&two).unwrap();
    let (mut first, _, _) = following_off_a_terminal(&checkout, &id, &one);
    let (mut second, _, _) = following_off_a_terminal(&checkout, &id, &two);
    assert_eq!(held_files(&checkout.home, &id), before);
    std::fs::write(marks.join("go"), "go").unwrap();
    assert_eq!(first.wait().unwrap().code(), Some(0));
    assert_eq!(second.wait().unwrap().code(), Some(0));

    let alone_root = tempfile::tempdir().unwrap();
    let alone_marks = alone_root.path().join("marks");
    let alone = holding(alone_root.path(), &alone_marks);
    std::fs::write(alone_marks.join("go"), "go").unwrap();
    let run = yunta_in!(&alone.repo, &alone.home, &["run", "wf.yaml"]);
    let alone_id = yunta_testkit::full_run_id(&alone.home, &run_id_from(&run));
    assert_eq!(kinds(&checkout.home, &id), kinds(&alone.home, &alone_id));
}
