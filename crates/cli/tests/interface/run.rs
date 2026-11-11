//! A run driven end to end: opened from the field and followed while this build drives
//! it, to its end and onto the successor a promotion hands the work to.

use super::*;

use std::process::{Child, Stdio};

use surco::doc::Block;
use surco::paint::Paint;
use tokio::sync::mpsc::UnboundedReceiver;
use yunta_testkit::{runs_root, wait_until, yunta_on_terminal, Checkout, Terminal};
use yunta_ui::News;

/// A node that says it started and holds until told to go, or until its marker is gone
/// with the test, so no engine outlives the test that started it; then one that
/// finishes.
fn holding(started: &Path, go: &Path) -> String {
    format!(
        "name: hold\nnodes:\n  - id: hold\n    kind: bash\n    run: \"echo in > {0}; until [ -f {1} ] || [ ! -e {0} ]; do sleep 0.05; done\"\n  - {{ id: after, kind: bash, run: \"true\", depends_on: [hold] }}\n",
        started.display(),
        go.display()
    )
}

/// A check that fails in either mode, with its reroute already spent: `quick` offers
/// to promote to `full`, which has nowhere later to go.
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

/// The lines of the chronicle on `state`'s screen, as a pipe writes them.
fn chronicle(state: &State) -> Vec<String> {
    let screen = state.screen(&Markdown.look());
    screen
        .main
        .doc
        .blocks()
        .iter()
        .flat_map(|block| match block {
            Block::Lines(lines) => lines.iter().map(|line| Paint::Plain.line(line)).collect(),
            _ => Vec::new(),
        })
        .filter(|line| line.starts_with('['))
        .collect()
}

/// `state` driven with `performer` from what `asked` asks for until it waits on
/// nothing, `followed` called once the following of a run hands it back as it reads
/// first.
async fn followed_through(
    state: &mut State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    mut followed: impl FnMut(),
) {
    let (inputs, mut received): (_, UnboundedReceiver<Input>) = mpsc::unbounded_channel();
    let mut asked = asked;
    loop {
        for request in asked.drain(..) {
            performer.take(request, &inputs);
        }
        if state.waiting_on().is_empty() {
            return;
        }
        let input = received
            .recv()
            .await
            .expect("the performer hands back what the state waits on");
        if matches!(input, Input::News(_, News::Following(_))) {
            followed();
        }
        asked = state.update(input);
    }
}

/// The runs under `home`, by their whole ids.
fn runs(home: &Path) -> Vec<String> {
    std::fs::read_dir(runs_root(home))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

/// A run of [`holding`] started off a terminal, held on its first node.
struct Held {
    checkout: Checkout,
    /// Where its markers are, and the follower's output.
    marks: tempfile::TempDir,
    /// The marker that lets it go on.
    go: PathBuf,
    id: String,
    process: Child,
}

impl Held {
    /// The run started, once its first node says so.
    fn started() -> Held {
        let marks = tempfile::tempdir().unwrap();
        let (started, go) = (marks.path().join("started"), marks.path().join("go"));
        let checkout = Checkout::new()
            .working_in_place()
            .workflow("wf", &holding(&started, &go))
            .committed();
        let process = checkout
            .command(Path::new(env!("CARGO_BIN_EXE_yunta")))
            .args(["run", "wf.yaml"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_until(|| started.exists(), || "the run never started".to_string());
        let id = runs(&checkout.home).remove(0);
        Held {
            checkout,
            marks,
            go,
            id,
            process,
        }
    }

    /// `yunta status <run> --follow`, what it writes going through a pipe into a file,
    /// once it has opened its lines: the follower, and where it writes.
    fn followed_through_a_pipe(&self) -> (Child, PathBuf) {
        let written = self.marks.path().join("followed.txt");
        let pipe = std::fs::File::create(&written).unwrap();
        let follower = self
            .checkout
            .command(Path::new(env!("CARGO_BIN_EXE_yunta")))
            .args(["status", &self.id, "--follow"])
            .stdin(Stdio::null())
            .stdout(pipe.try_clone().unwrap())
            .stderr(pipe)
            .spawn()
            .unwrap();
        let read = || std::fs::read_to_string(&written).unwrap_or_default();
        wait_until(
            || read().contains("one line per event"),
            || format!("the follower never opened its lines: {}", read()),
        );
        (follower, written)
    }
}

/// A run opened from the field is followed while it moves and to its end, and its
/// chronicle says each moment in the words `status --follow` writes through a pipe: the
/// lines that follower wrote close the chronicle, which keeps the run's whole story.
#[tokio::test]
async fn a_run_followed_to_its_end_says_each_moment_as_a_piped_follower_does() {
    let mut held = Held::started();
    let (mut follower, written) = held.followed_through_a_pipe();
    let checkout = &held.checkout;
    let performer = Arc::new(Performer::new(workspace(&checkout.repo, &checkout.home)));
    let (opened, asked) = State::opened(now());
    let (mut state, _) = drive(opened, asked, &performer, &[]).await;

    let asked = state.update(Input::Stroke(Stroke::Enter));
    followed_through(&mut state, asked, &performer, || {
        std::fs::write(&held.go, "").unwrap();
    })
    .await;

    assert!(held.process.wait().unwrap().success(), "the run finishes");
    assert!(
        follower.wait().unwrap().success(),
        "the follower ends with it"
    );
    let piped: Vec<String> = std::fs::read_to_string(&written)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with('['))
        .map(str::to_string)
        .collect();
    let told = chronicle(&state);
    assert!(!piped.is_empty(), "the follower wrote what came");
    assert!(told.ends_with(&piped), "{told:#?}\nends with\n{piped:#?}");
    let screen = state.screen(&Markdown.look());
    assert!(screen.side.is_some(), "the run's page stands beside it");
}

/// Moves the card on `terminal` down to `option` and chooses it.
fn choose(terminal: &mut Terminal, below: &[&str], option: &str) {
    for next in below.iter().chain([&option]) {
        if !terminal.screen().holds(&format!("▸ {next}")) {
            terminal.keys("\u{1b}[B");
        }
        terminal.wait_on_screen(&format!("▸ {next}"), "the card never pointed there");
    }
    terminal.keys("\r");
}

/// The run on `attached` answered at its terminal on a thread of its own, once
/// `followed` says the application follows it: promoted, then the question its
/// successor asks aborted. What the thread hands back is the code the run exits with.
fn answered_at_its_terminal(
    mut attached: Terminal,
    followed: std::sync::mpsc::Receiver<()>,
) -> std::thread::JoinHandle<Option<i32>> {
    std::thread::spawn(move || {
        followed.recv().unwrap();
        choose(&mut attached, &["abort"], "promote");
        attached.wait_on_screen("▸ retry", "the successor never asked");
        choose(&mut attached, &[], "abort");
        attached.ended();
        attached.exit_code()
    })
}

/// A run promoted while the application follows it is followed onto the run that
/// succeeds it, whose page is read in its place.
#[tokio::test]
async fn a_promotion_is_followed_onto_its_successor() {
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", PROMOTABLE)
        .committed();
    let attached = yunta_on_terminal!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml", "--mode", "quick"]
    );
    attached.wait_on_screen("▸ retry", "the exhausted reroute never asked");
    let first = runs(&checkout.home).remove(0);
    let (following, followed) = std::sync::mpsc::channel();
    let answering = answered_at_its_terminal(attached, followed);
    let performer = Arc::new(Performer::new(workspace(&checkout.repo, &checkout.home)));
    let (opened, asked) = State::opened(now());
    let (mut state, _) = drive(opened, asked, &performer, &[]).await;

    let asked = state.update(Input::Stroke(Stroke::Enter));
    // Only the first following starts the answers: the successor's comes after them.
    let mut told = false;
    followed_through(&mut state, asked, &performer, || {
        if !std::mem::replace(&mut told, true) {
            following.send(()).unwrap();
        }
    })
    .await;

    assert_eq!(
        answering.join().unwrap(),
        Some(1),
        "abort fails the successor"
    );
    let successor = runs(&checkout.home)
        .into_iter()
        .find(|run| *run != first)
        .expect("a successor");
    let screen = state.screen(&Markdown.look());
    assert_eq!(
        screen.trail.last().map(String::as_str),
        Some(yunta_testkit::handle(&successor))
    );
    let page = Markdown.screen(&screen);
    assert!(page.contains("promotable (full)"), "{page}");
}
