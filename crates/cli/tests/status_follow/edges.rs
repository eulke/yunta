//! A follower at the edges of a run: a promotion it follows onto the run that
//! succeeds it, a parent with a child open, and followers that change nothing.

use super::*;

/// A check that fails in either mode, with its reroute already spent: `quick`
/// offers to promote to `full`, which has nowhere later to go.
const PROMOTABLE: &str = r#"
name: promotable
modes:
  quick: { include: [check, fix] }
  full:  { include: all }
nodes:
  - id: check
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix, max_reroutes: 0 }
  - id: fix
    kind: bash
    run: "touch fixed.txt"
"#;

/// Moves the card on `terminal` down to `option` and chooses it.
fn choose(terminal: &mut yunta_testkit::Terminal, below: &[&str], option: &str) {
    for next in below.iter().chain([&option]) {
        if !terminal.screen().holds(&format!("▸ {next}")) {
            terminal.keys("\u{1b}[B");
        }
        terminal.wait_on_screen(&format!("▸ {next}"), "the card never pointed there");
    }
    terminal.keys("\r");
}

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
/// Reached by: `crates/cli/src/commands/status/follow.rs`, `crates/api/src/run/follow/following.rs`
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

    choose(&mut attached, &["abort"], "promote");
    attached.wait_on_screen("▸ retry", "the successor never asked");
    choose(&mut attached, &[], "abort");
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
/// Reached by: `crates/cli/src/commands/status/follow.rs`, `crates/api/src/run/follow/following.rs`
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
