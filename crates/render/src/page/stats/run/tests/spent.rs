//! Where a run's time went and who spent it, as the design draws them: under their
//! headings, the time a bar across the line and its parts each opening with its mark,
//! a role a row with its bar in one column; and the design's own run, read from its kept
//! log, as its targets draw it.

use super::*;

/// Where a run's time went and who spent it, each under its heading: the time a bar
/// across the line's whole width, then each part opening with its mark; a role a row,
/// the one that spent the most first, each bar against it standing in one column after
/// the names, and its tokens after the bar's track.
#[test]
fn where_its_time_went_and_who_spent_it_read_under_their_headings() {
    let (run, frames) = asked_run(2);
    let run = RunStats {
        time: yunta_replay::TimeSpent {
            working: Duration::from_secs(4_358),
            people: Duration::from_secs(2_391),
            ..Default::default()
        },
        ..run
    };
    let lines = lines_at(&run, &frames, Depth::Glance, &unicode(120));
    let bar = "█".repeat(120);
    assert_eq!(
        rows_under(&lines, "where its time went"),
        [bar.as_str(), "● working 1h12m · ● waiting on you 39m51s"]
    );
    let most = format!("implementer   {}  15.4k tokens", "█".repeat(28));
    let less = format!(
        "reviewer      {}{}  4.4k tokens",
        "█".repeat(8),
        " ".repeat(20)
    );
    assert_eq!(
        rows_under(&lines, "who spent it"),
        [most.as_str(), less.as_str()]
    );
}

/// The design's run, as it is kept beside the application's tests.
fn designed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../testkit-ui/fixtures/designed")
}

/// Where the design's run stands when its numbers are read: waiting on the person at its
/// last gate, `ship`, where the design draws them.
const AT_SHIP: u64 = 263;

/// The design's run, its log cut at `seq`, framed and counted at its last event in the
/// workflow its manifest froze: what the page reads of it.
fn designed_at(seq: u64) -> (RunFrame, RunStats) {
    let events: Vec<yunta_core::events::StoredEvent> =
        std::fs::read_to_string(designed().join("log.jsonl"))
            .expect("the design's run is kept")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a logged event"))
            .filter(|event: &yunta_core::events::StoredEvent| event.seq.get() <= seq)
            .collect();
    let frozen = std::fs::read(designed().join("run/manifest.yaml")).expect("its manifest");
    let workflow = yunta_core::Manifest::read_frozen(&frozen)
        .expect("the design's manifest reads")
        .doc
        .workflow;
    let state = yunta_replay::RunState::replay(&events);
    let now = state.clock().last_at().expect("a run with events");
    let engine = yunta_replay::registry::Engine::Unrecorded;
    let frame = RunFrame::of(&events[0].run_id, &workflow, &state, None, &engine, now);
    (frame, RunStats::at(&state, &workflow, now))
}

/// The design's run, waiting on the person at its last gate, reads as its targets draw
/// it, at the width each target's region takes, with what the printed page adds under
/// it: its nodes.
#[test]
fn the_design_s_run_reads_as_its_targets_draw_it() {
    let (frame, stats) = designed_at(AT_SHIP);
    let page = Stats {
        frame: &frame,
        stats: &stats,
        pricing: None,
        told: StatsTold::Printed(Depth::Glance),
    };
    let goldens: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/stats");
    for (size, glyphs, cells) in [
        ("unicode-120", Glyphs::Unicode, 118),
        ("ascii-120", Glyphs::Ascii, 118),
        ("unicode-80", Glyphs::Unicode, 78),
        ("unicode-60", Glyphs::Unicode, 58),
    ] {
        let look = Look {
            glyphs,
            paint: surco::paint::Paint::Plain,
            width: surco::look::Width::exact(cells),
            motion: surco::motion::Motion::Moves,
        };
        assert_golden(
            &goldens.join(format!("designed.{size}.txt")),
            &surco::surface::Surface::draw(&Terminal::on(look), &page.document(&look)),
        );
    }
}

/// On a run's screen the design's run tells its facts, where its time went and who spent it,
/// and leaves to the printed page what only it has room for: the answers given and its nodes.
#[test]
fn on_a_screen_the_design_s_run_leaves_the_printed_page_s_rows_to_it() {
    let (frame, stats) = designed_at(AT_SHIP);
    let told = |told| {
        let page = Stats {
            frame: &frame,
            stats: &stats,
            pricing: None,
            told,
        };
        let look = unicode(120);
        surco::surface::Surface::draw(&Terminal::on(look), &page.document(&look))
    };
    let printed = told(StatsTold::Printed(Depth::Glance));
    let on_screen = told(StatsTold::OnScreen);
    let opening = |page: &str, word: &str| {
        let words = |line: &str| line.split_whitespace().next() == Some(word);
        page.lines().any(words)
    };
    for printed_alone in ["answers", "nodes:"] {
        assert!(opening(&printed, printed_alone), "{printed}");
        assert!(!opening(&on_screen, printed_alone), "{on_screen}");
    }
    let last = on_screen.lines().last().map(str::split_whitespace);
    let last: Vec<&str> = last.into_iter().flatten().collect();
    assert_eq!(last, ["mechanical", "▎", "217k", "tokens"]);
}
