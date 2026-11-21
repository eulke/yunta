//! The run's whole chronicle, through `State` alone, on the design's run with `implement` at
//! work: the palette's chronicle row opens it over the run, a feed of its own of every
//! moment of every node, its newest moment on its last row; ten more moments keep it at its
//! end; scrolled back it says how many stand under it, and `End` takes it back to its end.
//! Opened on following the run, the application walks to it through the run's screen.

use surco::components::ScreenLaid;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_core::step::NextStep;
use yunta_testkit_ui::sandbox::now;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Stage, State};

/// Where the design's run stands ten moments past the target `chronicle`'s cut, at seq 104:
/// `implement`'s task asking for a path outside its scope, the run still at work.
const TEN_MOMENTS_ON: u64 = 114;

/// The rows a terminal gives the chronicle's region: its heading, the row that counts the
/// moments above its window, and six of them.
const ROWS: usize = 8;

/// The newest moment of the design's run at the cut, as the chronicle says it after its
/// subject.
const NEWEST_AT_THE_CUT: &str = "implement call yunta_request_scope_expansion";

/// The newest moment ten moments on.
const NEWEST_TEN_ON: &str =
    "implement global-pack-install asks for crates/cli/src/commands/pack_audit.rs";

/// A terminal 80 cells wide, in Unicode glyphs, unpainted.
fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(80),
        motion: Motion::Still,
    }
}

/// `line`'s words, one space between two.
fn worded(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `screen`'s main region laid out in [`ROWS`] rows, what was laid out handed back as a
/// terminal hands it back: each row's words.
fn laid_out(screen: &mut RunScreen) -> Vec<String> {
    let shown = screen.state().screen(&look());
    let paned = Terminal::on(look()).pane(&shown.main, ROWS);
    screen.laid(ScreenLaid {
        main: paned.laid,
        side: None,
        overlay: None,
    });
    paned
        .lines
        .iter()
        .map(|line| worded(&line.text()))
        .collect()
}

/// `strokes` pressed on `screen` one at a time, its main region laid out after each.
async fn pressed(screen: &mut RunScreen, strokes: &[Stroke]) -> Vec<String> {
    let mut lines = laid_out(screen);
    for stroke in strokes {
        screen.pressed(&[*stroke]).await;
        lines = laid_out(screen);
    }
    lines
}

#[tokio::test]
async fn the_palettes_chronicle_row_opens_it_at_its_end_and_it_stays_there_as_it_grows() {
    let mut screen = run::opened("chronicle").await;
    let shown = screen.state().screen(&look());
    assert_eq!(
        shown.trail,
        ["yunta", "Q78GRR chronicle"],
        "the palette's chronicle row opens the run's whole chronicle"
    );
    assert!(shown.overlay.is_none(), "chosen, the palette closes");

    let lines = laid_out(&mut screen);
    assert_eq!(
        lines.first().map(String::as_str),
        Some("its whole chronicle · 104 moments")
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some(NEWEST_AT_THE_CUT),
        "its newest moment on its last row: {lines:#?}"
    );

    screen.went_on(TEN_MOMENTS_ON).await;
    let lines = laid_out(&mut screen);
    assert_eq!(
        lines.first().map(String::as_str),
        Some("its whole chronicle · 114 moments")
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some(NEWEST_TEN_ON),
        "ten more moments keep it at its end: {lines:#?}"
    );

    // Its window of seven rows under the heading holds, at its end, the count above and the
    // six newest; three rows back, the count above, five rows and the count under: the six
    // and the three less the five stand under it.
    let back = pressed(&mut screen, &[Stroke::Up, Stroke::Up, Stroke::Up]).await;
    assert_eq!(
        back.last().map(String::as_str),
        Some("… 4 under"),
        "scrolled back, it counts what stands under it: {back:#?}"
    );
    let again = pressed(&mut screen, &[Stroke::End]).await;
    assert_eq!(
        again.last().map(String::as_str),
        Some(NEWEST_TEN_ON),
        "`End` takes it back to its end: {again:#?}"
    );
}

/// Opened on following a run, the application has yet to open the run's screen and the
/// run's whole chronicle over it, as a person walks there; going back walks the same way.
#[test]
fn opened_on_following_a_run_it_walks_through_the_runs_screen_to_its_chronicle() {
    let run = "01M7THISRUN000000000000000".parse().expect("a run's id");
    let (state, _) =
        State::opened_on(now(), &NextStep::Follow { run }).expect("a stage takes the step");
    let walked: Vec<&str> = state
        .stages_to_open()
        .iter()
        .map(|stage| match stage {
            Stage::Run(_) => "the run's screen",
            Stage::Chronicle(_) => "the run's whole chronicle",
            _ => "another stage",
        })
        .collect();
    assert_eq!(walked, ["the run's screen", "the run's whole chronicle"]);
}
