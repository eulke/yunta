//! `yunta status <run> --follow`: a run followed from a process that did not
//! start it, as a person at another terminal or a script reads it.
//!
//! What it shows of a run that moves is what `yunta run` shows; what it shows of
//! one that does not is what `yunta status` shows; and it exits with the code
//! the run's stop gives, so a script reads the outcome as it reads `yunta run`'s.

use std::path::{Path, PathBuf};

use yunta_testkit::{
    run_id_from, runs_root, stdout, unwrapped, wait_until, yunta_in, yunta_on_terminal, Checkout,
};

#[path = "status_follow/edges.rs"]
mod edges;

#[path = "status_follow/said.rs"]
mod said;

/// A gate after one node: the run parks on it off a terminal, with exit 3.
const GATED: &str = "\
name: gated
nodes:
  - { id: first, kind: bash, run: \"true\" }
  - { id: approve, kind: gate, depends_on: [first], assignee: lead, message: \"Go on?\", options: [go] }
";

/// One node, which finishes.
const FINISHED: &str = "\
name: quick
nodes:
  - { id: only, kind: bash, run: \"true\" }
";

/// A repository with `workflow` committed as `wf.yaml`, and a run of it driven
/// off a terminal to its stop: the checkout and the run's id.
fn ran(root: &std::path::Path, workflow: &str) -> (Checkout, String) {
    let checkout = Checkout::under(root)
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", workflow)
        .committed();
    let run = yunta_in!(&checkout.repo, &checkout.home, &["run", "wf.yaml"]);
    let id = run_id_from(&run);
    (checkout, id)
}

/// A page with what moves between two readings of it taken out: how long the
/// run has stood where it stands, which the headline counts in seconds.
fn steady(page: &str) -> String {
    page.lines()
        .map(|line| match line.find(" · for ") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A run that is not moving has nothing to follow: the page is the one `status`
/// prints, and the exit code is the one its stop gives.
#[test]
fn following_a_run_that_is_not_moving_prints_its_page_and_exits_with_its_code() {
    let root = tempfile::tempdir().unwrap();
    let (checkout, id) = ran(root.path(), GATED);

    let page = yunta_in!(&checkout.repo, &checkout.home, &["status", &id]);
    assert_eq!(
        page.status.code(),
        Some(0),
        "status alone reports and exits 0"
    );
    let followed = yunta_in!(&checkout.repo, &checkout.home, &["status", &id, "--follow"]);
    assert_eq!(followed.status.code(), Some(3), "{}", stdout(&followed));
    assert_eq!(steady(&stdout(&followed)), steady(&stdout(&page)));

    let root = tempfile::tempdir().unwrap();
    let (checkout, id) = ran(root.path(), FINISHED);
    let followed = yunta_in!(&checkout.repo, &checkout.home, &["status", &id, "--follow"]);
    assert_eq!(followed.status.code(), Some(0), "{}", stdout(&followed));
}

/// What `--follow` draws is the run moving, which none of these readings is.
#[test]
fn follow_is_refused_beside_a_document_a_depth_or_a_node() {
    let root = tempfile::tempdir().unwrap();
    let (checkout, id) = ran(root.path(), GATED);
    for beside in [
        vec!["--json"],
        vec!["--full"],
        vec!["--dig"],
        vec!["--node", "first"],
    ] {
        let mut args = vec!["status", id.as_str(), "--follow"];
        args.extend(&beside);
        let refused = yunta_in!(&checkout.repo, &checkout.home, &args);
        assert_eq!(refused.status.code(), Some(2), "{beside:?}");
    }
}

/// A run held on its middle node until the test writes `go` under `marks`, or
/// until `marks` is gone with the test, so no engine outlives the test that
/// started it. The node says it started by writing `started` there.
fn held(marks: &Path) -> String {
    let marks = marks.display();
    format!(
        "name: held\n\
         nodes:\n  \
         - {{ id: first, kind: bash, run: \"true\" }}\n  \
         - {{ id: hold, kind: bash, depends_on: [first], run: \"touch {marks}/started; until [ -f {marks}/go ] || [ ! -d {marks} ]; do sleep 0.05; done\" }}\n  \
         - {{ id: last, kind: bash, depends_on: [hold], run: \"true\" }}\n"
    )
}

/// A repository whose workflow is [`held`] on `marks`.
fn holding(root: &Path, marks: &Path) -> Checkout {
    std::fs::create_dir_all(marks).unwrap();
    Checkout::under(root)
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", &held(marks))
        .committed()
}

/// The one run under `home`, once its held node has started.
fn the_held_run(home: &Path, marks: &Path) -> String {
    wait_until(
        || marks.join("started").exists(),
        || "the held node never started".to_string(),
    );
    let runs: Vec<PathBuf> = std::fs::read_dir(runs_root(home))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    runs[0].file_name().unwrap().to_string_lossy().into_owned()
}

/// `text` without the sequences that move a cursor, clear a row or name a
/// window: what is left is what a person reads.
fn without_sequences(text: &str) -> String {
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            plain.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() || c == '~' {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    plain
}

/// The block a drawing ends on, from its headline, which names the workflow and
/// says the run finished, as a person reads it. From the headline and not from
/// the start of its line: a region's last redraws, which no two terminals take
/// alike, end on that line once the sequences that placed them are gone.
fn closing_block(drawn: &str) -> String {
    let plain = without_sequences(drawn);
    let at = plain
        .rfind(" (default) · finished ·")
        .unwrap_or_else(|| panic!("no closing block in:\n{plain}"));
    let named = plain[..at].rfind(' ').map_or(0, |space| space + 1);
    unwrapped(&plain[named..])
}

/// One run, driven at one terminal and followed at another: what each is left
/// with when it stops is the same block, and the same exit code.
///
/// Reached by: `crates/cli/src/commands/status/follow.rs`, `crates/cli/src/surface/mod.rs`
#[test]
fn a_follower_leaves_the_block_and_the_code_the_attached_run_leaves() {
    let root = tempfile::tempdir().unwrap();
    let marks = root.path().join("marks");
    let checkout = holding(root.path(), &marks);
    let mut attached = yunta_on_terminal!(&checkout.repo, &checkout.home, &["run", "wf.yaml"]);
    let id = the_held_run(&checkout.home, &marks);

    let mut follower =
        yunta_on_terminal!(&checkout.repo, &checkout.home, &["status", &id, "--follow"]);
    follower.wait_for("hold", "the follower never drew the held node");
    std::fs::write(marks.join("go"), "go").unwrap();

    let driven = attached.ended();
    let followed = follower.ended();
    assert_eq!(closing_block(&followed), closing_block(&driven));
    assert_eq!(attached.exit_code(), Some(0));
    assert_eq!(follower.exit_code(), attached.exit_code());
}

/// A follower of the run `id` off a terminal, once it has said why it writes
/// lines: the process, and the files its stdout and its stderr go to.
fn following_off_a_terminal(
    checkout: &Checkout,
    id: &str,
    root: &Path,
) -> (std::process::Child, PathBuf, PathBuf) {
    let (out, err) = (root.join("out"), root.join("err"));
    let mut follower = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    yunta_testkit::hermetic(&mut follower, &checkout.repo, &checkout.home);
    let follower = follower
        .args(["status", id, "--follow"])
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(&out).unwrap())
        .stderr(std::fs::File::create(&err).unwrap())
        .spawn()
        .unwrap();
    let said = || std::fs::read_to_string(&err).unwrap_or_default();
    wait_until(
        || said().contains("not a terminal"),
        || format!("the follower never said why it writes lines: {}", said()),
    );
    (follower, out, err)
}

/// Off a terminal a follower says why it writes lines, writes one for each
/// moment after the one it started on, and closes as the run would.
#[test]
fn off_a_terminal_a_follower_prints_a_line_per_moment() {
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

    let (mut follower, out, err) = following_off_a_terminal(&checkout, &id, root.path());
    let said = || std::fs::read_to_string(&err).unwrap_or_default();
    std::fs::write(marks.join("go"), "go").unwrap();

    let ended = follower.wait().unwrap();
    assert_eq!(ended.code(), Some(0), "{}", said());
    assert!(
        said().contains("last"),
        "the moments after it began: {}",
        said()
    );
    let closed = std::fs::read_to_string(&out).unwrap();
    assert!(closed.contains("· finished ·"), "{closed}");
}

/// Whether the run `id`'s log, read from the store, holds a pause or a resume:
/// a run that neither paused nor resumed was driven by one engine throughout.
fn paused_or_resumed(home: &Path, id: &str) -> bool {
    use yunta_core::events::{EventPayload, RunEvent};
    yunta_storage::Storage::open(&home.join("yunta.db"))
        .unwrap()
        .events_for_run(&yunta_core::RunId::from(id))
        .unwrap()
        .iter()
        .any(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Run(
                    RunEvent::Paused(_) | RunEvent::Resumed(_)
                ))
            )
        })
}

/// Ctrl-C at a follower stops the follower and nothing else: it says the run
/// goes on and how to follow it again, it exits 130, and the run, which another
/// process drives, finishes when it is let go.
///
/// Reached by: `crates/cli/src/commands/status/follow.rs`
#[test]
fn ctrl_c_stops_following_and_the_run_goes_on() {
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
    let mut follower =
        yunta_on_terminal!(&checkout.repo, &checkout.home, &["status", &id, "--follow"]);
    follower.wait_for("hold", "the follower never drew the held node");

    follower.interrupt();
    let said = unwrapped(&without_sequences(&follower.ended()));
    assert_eq!(follower.exit_code(), Some(130), "{said}");
    let again = format!("yunta status {} --follow", yunta_testkit::handle(&id));
    assert!(said.contains(&again), "no `{again}` in: {said}");
    assert!(said.contains("goes on"), "{said}");

    std::fs::write(marks.join("go"), "go").unwrap();
    wait_until(
        || {
            let status = yunta_in!(&checkout.repo, &checkout.home, &["status", &id, "--json"]);
            stdout(&status).contains("\"outcome\": \"finished\"")
        },
        || "the run never finished once let go".to_string(),
    );
    assert!(!paused_or_resumed(&checkout.home, &id));
}

/// Kills the engine the registry of the run `id` names, as `kill -9` does, and
/// waits for it to be gone.
fn kill_the_engine_of(home: &Path, id: &str) {
    let registry = runs_root(home).join(id).join("scratch").join("engine.json");
    let engine: serde_json::Value =
        serde_json::from_slice(&std::fs::read(registry).unwrap()).unwrap();
    let pid = engine["engine_pid"].to_string();
    let signal = |args: &[&str]| {
        std::process::Command::new("kill")
            .args(args)
            .status()
            .unwrap()
    };
    assert!(signal(&["-9", &pid]).success());
    wait_until(
        || !signal(&["-0", &pid]).success(),
        || "the killed engine is still there".into(),
    );
}

/// A run whose log says it moves while no engine drives it has stalled, which
/// `yunta run` never meets because it is that engine. The follower ends with
/// what `status` says of such a run, exit 1, within two seconds of the death.
///
/// Reached by: `crates/cli/src/commands/status/follow.rs`
#[test]
fn an_engine_killed_under_a_follower_ends_it_as_a_stall() {
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
    let (mut follower, out, _) = following_off_a_terminal(&checkout, &id, root.path());

    kill_the_engine_of(&checkout.home, &id);
    let killed = std::time::Instant::now();
    let ended = yunta_testkit::wait_for(
        || follower.try_wait().unwrap(),
        || "the follower never ended once its run's engine was gone".to_string(),
    );
    let waited = killed.elapsed();
    let said = std::fs::read_to_string(&out).unwrap();
    assert_eq!(ended.code(), Some(1), "{said}");
    assert!(waited < std::time::Duration::from_secs(2), "{waited:?}");
    assert!(said.contains("no process is driving it"), "{said}");
}

/// A gate after one node, asked at the terminal of the engine that meets it.
const ASKS: &str = "\
name: asks
nodes:
  - { id: first, kind: bash, run: \"true\" }
  - { id: ship, kind: gate, depends_on: [first], assignee: lead, message: \"Ship it?\", options: [approve] }
";

/// A question asked live is on no log until it is answered, so the follower
/// reads it off the run's registry: it shows the run asking at its terminal,
/// and once the question is answered there, follows the run to its end.
#[test]
fn a_run_asking_at_its_terminal_is_shown_asking_and_followed_on() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", ASKS)
        .committed();
    let mut attached = yunta_on_terminal!(&checkout.repo, &checkout.home, &["run", "wf.yaml"]);
    attached.wait_on_screen("▸ approve", "the gate never put its card on the console");
    let runs: Vec<_> = std::fs::read_dir(runs_root(&checkout.home))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let id = runs[0].clone();

    let mut follower =
        yunta_on_terminal!(&checkout.repo, &checkout.home, &["status", &id, "--follow"]);
    follower.wait_for(
        "node `ship` asks at its terminal",
        "the follower never said the run asks at its terminal",
    );
    attached.keys("\r");
    attached.ended();
    let followed = follower.ended();
    assert_eq!(follower.exit_code(), Some(0), "{followed}");
    assert!(closing_block(&followed).contains("· exit 0"), "{followed}");
}
