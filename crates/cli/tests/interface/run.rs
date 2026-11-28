//! A run driven end to end: opened from the field and followed while this build drives
//! it, to its end and onto the successor a promotion hands the work to.

use super::*;

use std::collections::BTreeSet;
use std::process::{Child, Stdio};

use surco::doc::Block;
use surco::paint::Paint;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use yunta_api::FollowNews;
use yunta_testkit::workflows::PROMOTABLE;
use yunta_testkit::{run_ids, wait_until, yunta_on_terminal, Checkout, HeldRun, Terminal};
use yunta_ui::Input;

/// The run's whole chronicle over `state`'s run, opened from the palette by typing what its
/// row does: each row as a pipe writes it, its subject and its words, the newest last, and
/// how many moments the whole chronicle holds.
fn chronicle(state: &State) -> (Vec<(String, String)>, usize) {
    let mut opened = state.clone();
    for stroke in std::iter::once('/').chain("chronicle".chars()) {
        opened.update(Input::Stroke(Stroke::Insert(stroke)));
    }
    let asked = opened.update(Input::Stroke(Stroke::Enter));
    assert!(
        asked.is_empty(),
        "the chronicle opens reading nothing: {asked:?}"
    );
    let screen = opened.screen(&Markdown.look());
    let Some(Block::Feed(feed)) = screen.main.doc.blocks().last() else {
        panic!("the chronicle stands as a feed: {:?}", screen.trail);
    };
    assert_eq!(feed.after, 0, "it opens at its end");
    let rows = feed
        .rows
        .iter()
        .map(|row| {
            let subject = row.subject.clone().unwrap_or_default();
            (subject, Paint::Plain.line(&row.line))
        })
        .collect();
    (rows, feed.before + feed.rows.len())
}

/// The lines a pipe wrote, `piped`, close the run's whole chronicle over `state`'s run, each
/// the row that stands with it from the chronicle's end: the line's subject, and its words
/// after them.
fn closes_the_chronicle(state: &State, piped: &[String]) {
    let (told, whole) = chronicle(state);
    assert!(
        !told.is_empty(),
        "the chronicle stands at its end, its newest rows in sight"
    );
    assert!(whole >= piped.len(), "{whole} moments told, {piped:#?}");
    let closing = piped.len().min(told.len());
    let pairs = told[told.len() - closing..]
        .iter()
        .zip(&piped[piped.len() - closing..]);
    for ((subject, row), line) in pairs {
        let after = after_its_subject(line, subject);
        assert_eq!(
            after.map(worded),
            Some(worded(row)),
            "the chronicle's row `{subject}  {row}` against `{line}`"
        );
    }
}

/// What `line`, a line a pipe writes, says after its time, its mark and `subject`, and what
/// joins them: none where it is not about `subject`.
fn after_its_subject<'l>(line: &'l str, subject: &str) -> Option<&'l str> {
    let (_, said) = line.split_once("] ")?;
    let marked = said.split_once(' ').map(|(_, after)| after);
    let words = said
        .strip_prefix(subject)
        .or_else(|| marked?.strip_prefix(subject))?;
    Some(words.strip_prefix(':').unwrap_or(words))
}

/// The rows of the feed `state`'s pane ends with, as a pipe writes them: none where its pane
/// ends with none.
fn feed_rows(state: &State) -> Vec<String> {
    let screen = state.screen(&Markdown.look());
    match screen.main.doc.blocks().last() {
        Some(Block::Feed(feed)) => feed
            .rows
            .iter()
            .map(|row| Paint::Plain.line(&row.line))
            .collect(),
        _ => Vec::new(),
    }
}

/// `state` driven with `performer` from what `asked` asks for until it waits on
/// nothing, the run in view no longer moves, and it stands caught up with the log of the
/// store `performer` follows; `followed` called each time what it follows hands a run
/// read whole, its first reading. Each row of the feed its pane ends with on the way is
/// kept in `fed`.
async fn followed_through(
    state: &mut State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    fed: &mut BTreeSet<String>,
    mut followed: impl FnMut(),
) {
    let (inputs, mut received): (_, UnboundedReceiver<Input>) = mpsc::unbounded_channel();
    let store = performer
        .workspace()
        .project()
        .expect("the performer's project names the store it follows")
        .storage_path;
    let mut asked = asked;
    loop {
        for request in asked.drain(..) {
            performer.take(request, &inputs);
        }
        let in_view = state.followed_runs().in_view.as_ref();
        let moves = in_view.is_some_and(|in_view| in_view.run.frame().moves());
        let at_rest = state.waiting_on().is_empty() && !moves;
        if at_rest && state.caught_up(&yunta_testkit::heads(&store)) {
            return;
        }
        let input = received
            .recv()
            .await
            .expect("the performer hands back what the state waits on");
        if matches!(input, Input::FollowNews(FollowNews::Whole { .. })) {
            followed();
        }
        asked = state.update(input);
        fed.extend(feed_rows(state));
    }
}

/// `line`'s words, one space between two: a feed's call stands its verb in a column, where
/// a pipe parts it from its object by a space.
fn worded(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `yunta status <run> --follow` on `held`, what it writes going through a pipe into a
/// file beside its markers, once it has opened its lines: the follower, and where it
/// writes.
fn followed_through_a_pipe(held: &HeldRun) -> (Child, PathBuf) {
    let written = held.hold.marks().join("followed.txt");
    let pipe = std::fs::File::create(&written).unwrap();
    let follower = held
        .checkout
        .command(Path::new(env!("CARGO_BIN_EXE_yunta")))
        .args(["status", &held.id, "--follow"])
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

/// A run opened from the field is followed while it moves and to its end, and its node's
/// feed and its whole chronicle say each moment in the words `status --follow` writes through
/// a pipe: each row of the feed the node's pane ends with while it works is a line that
/// follower wrote, after its time and its subject; the lines it wrote close the chronicle the
/// palette opens over the run, each row the line's subject and its words after them, which
/// keeps the run's whole story.
#[tokio::test]
async fn a_run_followed_to_its_end_says_each_moment_as_a_piped_follower_does() {
    let (held, mut process) = HeldRun::started(Path::new(env!("CARGO_BIN_EXE_yunta")));
    let (mut follower, written) = followed_through_a_pipe(&held);
    let checkout = &held.checkout;
    let performer = Arc::new(Performer::new(
        workspace(&checkout.repo, &checkout.home),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let (mut state, _) = drive(opened, asked, &performer, &[]).await;

    let asked = state.update(Input::Stroke(Stroke::Enter));
    let mut fed = BTreeSet::new();
    followed_through(&mut state, asked, &performer, &mut fed, || {
        held.hold.let_go()
    })
    .await;

    assert!(process.wait().unwrap().success(), "the run finishes");
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
    assert!(!piped.is_empty(), "the follower wrote what came");
    assert!(!fed.is_empty(), "the node's feed stood while it worked");
    for row in &fed {
        let after_its_subject = format!(" {}", worded(row));
        assert!(
            piped
                .iter()
                .any(|line| worded(line).ends_with(&after_its_subject)),
            "the feed's row `{row}` among\n{piped:#?}"
        );
    }
    closes_the_chronicle(&state, &piped);
    let screen = state.screen(&Markdown.look());
    assert!(
        screen.side.is_some(),
        "the run's nodes stand beside its receipt"
    );
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
        attached.choose(&["abort"], "promote");
        attached.wait_on_screen("▸ retry", "the successor never asked");
        attached.choose(&[], "abort");
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
    let first = run_ids(&checkout.home).remove(0);
    let (following, followed) = std::sync::mpsc::channel();
    let answering = answered_at_its_terminal(attached, followed);
    let performer = Arc::new(Performer::new(
        workspace(&checkout.repo, &checkout.home),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let (mut state, _) = drive(opened, asked, &performer, &[]).await;

    let asked = state.update(Input::Stroke(Stroke::Enter));
    // Only the first following starts the answers: the successor's comes after them.
    let mut told = false;
    followed_through(&mut state, asked, &performer, &mut BTreeSet::new(), || {
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
    let successor = run_ids(&checkout.home)
        .into_iter()
        .find(|run| *run != first)
        .expect("a successor");
    // The run's trail names its handle, then the node pointed at.
    let screen = state.screen(&Markdown.look());
    assert_eq!(
        screen.trail.get(1).map(String::as_str),
        Some(yunta_testkit::handle(&successor))
    );
    let page = Markdown.screen(&screen);
    assert!(page.contains("promotable (full)"), "{page}");
}
