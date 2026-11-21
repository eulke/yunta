//! A node's feed in the application at a terminal while its session works: a run whose
//! session is held between its calls, each let go by a word the test gives, followed with no
//! key pressed. Each call let go stands as the node's feed's last row, the pane's last row,
//! before the next word is given; scrolled back with the arrows, the rows in sight stay where
//! they stood as more calls are let go, and the count under them rises; the pane stays on
//! that node as the run moves on to the next; `End` brings the feed back to its newest row,
//! the node's end under its last call. The run's engine is `yunta run` in a process of its
//! own, since a run scripted from a fixture is never handed to a detached one. Each step
//! waits for the words the screen shows, and a key is sent once the screen it acts on stands.

use std::path::Path;
use std::process::{Child, Stdio};

use yunta_testkit::{
    run_ids, runs_root, wait_until, yunta_at, yunta_on_terminal_sized, Checkout, Hold, Terminal,
};

use super::{given_back, left_through_the_field, FOLLOWED, THE_FIELD};

/// The terminal the run is followed on: wide enough that its nodes stand beside its pane,
/// which they do from 102 columns, and as tall as the harness's.
const SIZE: (u16, u16) = (120, Terminal::ROWS);

/// The calls the session makes before its first word: more rows than the pane holds at
/// [`SIZE`], so the feed's newest row is the pane's last.
const BEFORE_THE_WORDS: usize = 16;

/// The calls the session makes one at a time, each once the test gives its word: three
/// before the feed is scrolled back, three while it is.
const HELD: usize = 6;

/// The calls let go before the feed is scrolled back.
const LET_GO_AT_ITS_END: usize = 3;

/// The rows the arrows scroll the feed back by.
const SCROLLED_BACK: usize = 3;

/// The word that lets the held session end once its last call was let go.
const DONE: &str = "done";

/// The word that lets the node after the held one finish.
const FINISH: &str = "finish";

/// The ribbon's strokes once the run reached `after`: each stretch under its name, `work` and
/// `after`, plowed.
const AFTER_REACHED: &str = "━━━━ ━━━━━";

/// What `work`'s feed says last once its session ended: its end, in the words the session's
/// summary gives it.
const ITS_END: &str = "finished: read";

/// What the meter says under every run's pane, the row the pane's last row stands over.
const THE_METER: &str = " at work";

/// The arrow up, as a terminal sends it.
const UP: &str = "\u{1b}[A";

/// `End`, as a terminal sends it.
const END: &str = "\u{1b}[F";

/// The runner the held node is worked by, on the mock.
const READER_ON_THE_MOCK: &str = "defaults:\n  isolation: none\nrunners:\n  reader:\n    \
                                  - { adapter: mock, model: mock-model }\n";

/// The binary these tests run, which runs the held run's engine too.
fn yunta() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_yunta"))
}

/// The note the `n`th call reads before the words, as the repository holds it.
fn read_first(n: usize) -> String {
    format!("notes/first-{n:02}.md")
}

/// The note the `n`th call let go by a word reads.
fn read_held(n: usize) -> String {
    format!("notes/held-{n}.md")
}

/// The word that lets the `n`th held call go.
fn word(n: usize) -> String {
    format!("call-{n}")
}

/// The workflow `reads`: `work`, a session that reads the notes, then `after`, which holds
/// until `hold` gives the word [`FINISH`], or until its marks are gone.
fn reads_then_holds(hold: &Hold) -> String {
    format!(
        "name: reads\nnodes:\n  - {{ id: work, kind: prompt, runner: reader, prompt: \"Read the notes.\" }}\n  - id: after\n    kind: bash\n    run: \"until [ -f {finish} ] || [ ! -e {marks} ]; do sleep 0.05; done\"\n    depends_on: [work]\n",
        finish = hold.word(FINISH).display(),
        marks = hold.marks().display(),
    )
}

/// The session `work` opens: [`BEFORE_THE_WORDS`] reads, then [`HELD`] more, each once
/// `hold` gives its word; then it is done once `hold` gives the word [`DONE`].
fn held_session(hold: &Hold) -> String {
    let first = (1..=BEFORE_THE_WORDS).map(|n| {
        format!(
            "      - {{ type: file_call, action: read, path: \"{}\" }}\n",
            read_first(n)
        )
    });
    let held = (1..=HELD).map(|n| {
        format!(
            "      - {{ type: awaits_word, word: \"{}\" }}\n      - {{ type: file_call, action: read, path: \"{}\" }}\n",
            hold.word(&word(n)).display(),
            read_held(n)
        )
    });
    let steps: String = first.chain(held).collect();
    let done = hold.word(DONE);
    format!(
        "dialect: claude-code\nsessions:\n  - steps:\n{steps}      - {{ type: awaits_word, word: \"{}\" }}\n    outcome: {{ type: completed, summary: read }}\n",
        done.display()
    )
}

/// What the pane shows on `rows`, a screen's rows: each row's part right of the edge of the
/// nodes beside it.
fn pane(rows: &[String]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            row.split_once('│')
                .map_or("", |(_, pane)| pane)
                .trim_end()
                .to_string()
        })
        .collect()
}

/// The pane's last row on `terminal`'s screen: the row over the meter's.
fn last_row(terminal: &Terminal) -> Option<String> {
    let rows = terminal.screen().rows;
    let meter = rows.iter().position(|row| row.contains(THE_METER))?;
    pane(&rows[..meter]).pop()
}

/// The feed's window as `terminal`'s screen shows it, from the row that counts the rows
/// above it to the last row in sight, and how many rows its last row counts under it.
fn window(terminal: &Terminal) -> Option<(Vec<String>, usize)> {
    let rows = pane(&terminal.screen().rows);
    let above = rows
        .iter()
        .position(|row| row.trim_start().starts_with("… ") && row.ends_with(" above"))?;
    let under = rows
        .iter()
        .position(|row| row.trim_start().starts_with("… ") && row.ends_with(" under"))?;
    let counted = rows[under]
        .trim_start()
        .strip_prefix("… ")?
        .strip_suffix(" under")?;
    Some((rows[above..under].to_vec(), counted.parse().ok()?))
}

/// Waits until `terminal`'s screen holds the feed's window counting `under` rows under it,
/// failing with `what` and the screen: the window then.
fn counting_under(terminal: &Terminal, under: usize, what: &str) -> Vec<String> {
    wait_until(
        || window(terminal).is_some_and(|(_, counted)| counted == under),
        || {
            format!(
                "{what}\non the screen:\n{}",
                terminal.screen().rows.join("\n")
            )
        },
    );
    window(terminal).map(|(rows, _)| rows).unwrap_or_default()
}

/// Waits until `terminal`'s screen holds the feed's window counting more than `under` rows
/// under it, failing with `what` and the screen: how many it counts then.
fn more_under(terminal: &Terminal, under: usize, what: &str) -> usize {
    wait_until(
        || window(terminal).is_some_and(|(_, counted)| counted > under),
        || {
            format!(
                "{what}\non the screen:\n{}",
                terminal.screen().rows.join("\n")
            )
        },
    );
    window(terminal).map_or(under, |(_, counted)| counted)
}

/// Waits until the pane's last row on `terminal` holds `said`, failing with `what` and the
/// screen.
fn last_row_holds(terminal: &Terminal, said: &str, what: &str) {
    wait_until(
        || last_row(terminal).is_some_and(|row| row.contains(said)),
        || {
            format!(
                "{what}\non the screen:\n{}",
                terminal.screen().rows.join("\n")
            )
        },
    );
}

/// The run of `reads_then_holds`, its session held by `hold`'s words, started by `yunta run`
/// in a process of its own in a repository that holds the notes it reads, once its log holds
/// it: the repository, and that process.
fn held_run(hold: &Hold) -> (Checkout, Child) {
    let notes = (1..=BEFORE_THE_WORDS)
        .map(read_first)
        .chain((1..=HELD).map(read_held));
    let checkout = Checkout::new()
        .config(READER_ON_THE_MOCK)
        .workflow("wf", &reads_then_holds(hold))
        .file("fixture.yaml", &held_session(hold));
    let checkout = notes
        .fold(checkout, |checkout, note| checkout.file(&note, "a note\n"))
        .committed();
    let arguments = [
        "run",
        "wf.yaml",
        "--adapter",
        "mock",
        "--fixture",
        "fixture.yaml",
    ];
    let engine = checkout
        .command(yunta())
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the run starts");
    wait_until(
        || runs_root(&checkout.home).is_dir() && !run_ids(&checkout.home).is_empty(),
        || "the run was never created".to_string(),
    );
    let id = run_ids(&checkout.home).remove(0);
    wait_until(
        || yunta_at!(&checkout, &["status", &id]).status.success(),
        || "the run was never logged".to_string(),
    );
    (checkout, engine)
}

/// Bare `yunta` at a terminal of [`SIZE`] in `checkout`'s repository, its one run opened from
/// the field and followed to where its session waits for its first word.
fn followed(checkout: &Checkout) -> Terminal {
    let mut terminal = yunta_on_terminal_sized!(&checkout.repo, &checkout.home, &[], SIZE);
    terminal.wait_on_screen(THE_FIELD, "bare yunta never opened on the field");
    terminal.keys("\r");
    terminal.wait_on_screen(FOLLOWED, "the run opened from the field was never followed");
    last_row_holds(
        &terminal,
        &read_first(BEFORE_THE_WORDS),
        "the calls before the first word never stood at the feed's end",
    );
    terminal
}

/// `terminal`'s feed scrolled back by the arrows, each sent once the last moved it: what it
/// counts under its window then, and the window.
fn scrolled_back(terminal: &mut Terminal) -> (usize, Vec<String>) {
    let mut back = 0;
    for _ in 0..SCROLLED_BACK {
        terminal.keys(UP);
        back = more_under(terminal, back, "the arrow never scrolled the feed back");
    }
    (
        back,
        counting_under(terminal, back, "the feed stands scrolled back"),
    )
}

/// The run on `terminal` moved on to `after` once `hold` lets its session end: the pane stays
/// on `work`, its rows in sight from the first as `in_sight` holds them.
fn moved_on(terminal: &Terminal, hold: &Hold, in_sight: &[String]) {
    hold.give(DONE);
    wait_until(
        || {
            let screen = terminal.screen();
            screen.holds("✓ work") && screen.holds(AFTER_REACHED)
        },
        || {
            format!(
                "the run never moved on from `work`\non the screen:\n{}",
                terminal.screen().rows.join("\n")
            )
        },
    );
    let rows = terminal.screen().rows;
    assert!(
        rows[0].contains("› work"),
        "the pane stays on `work`: {rows:#?}"
    );
    let (now, _) = window(terminal).expect("the feed still stands scrolled back");
    assert_eq!(
        now[..2],
        in_sight[..2],
        "the rows in sight stand as the run moved on"
    );
}

/// `End` pressed on `terminal`: the feed back at its newest row, the node's end, under the
/// last call, nothing counted under it.
fn brought_to_its_end(terminal: &mut Terminal) {
    terminal.keys(END);
    last_row_holds(
        terminal,
        ITS_END,
        "`End` never brought the feed back to its newest row",
    );
    let rows = terminal.screen().rows;
    let meter = rows.iter().position(|row| row.contains(THE_METER));
    let in_the_pane = pane(&rows[..meter.expect("the meter stands")]);
    assert!(
        window(terminal).is_none(),
        "nothing counted under: {in_the_pane:#?}"
    );
    assert!(
        in_the_pane[in_the_pane.len() - 2].contains(&read_held(HELD)),
        "the last call stands over the node's end: {in_the_pane:#?}"
    );
}

#[test]
fn a_held_sessions_calls_stand_last_in_its_feed_which_scrolled_back_keeps_its_rows_in_sight() {
    let hold = Hold::new();
    let (checkout, mut engine) = held_run(&hold);
    let mut terminal = followed(&checkout);
    for n in 1..=LET_GO_AT_ITS_END {
        hold.give(&word(n));
        let what = format!("call {n} let go never stood as the pane's last row");
        last_row_holds(&terminal, &read_held(n), &what);
    }

    let (back, in_sight) = scrolled_back(&mut terminal);
    for n in LET_GO_AT_ITS_END + 1..=HELD {
        hold.give(&word(n));
        let what = format!("call {n} let go was never counted under the rows in sight");
        let now = counting_under(&terminal, back + n - LET_GO_AT_ITS_END, &what);
        assert_eq!(
            now, in_sight,
            "call {n}: the rows in sight stand where they stood"
        );
    }

    moved_on(&terminal, &hold, &in_sight);
    brought_to_its_end(&mut terminal);
    hold.give(FINISH);
    left_through_the_field(&mut terminal, None);
    assert_eq!(given_back(&mut terminal), Some(0));
    let ended = engine.wait().expect("the run's engine ends");
    assert!(ended.success(), "the run finishes: {ended}");
}
