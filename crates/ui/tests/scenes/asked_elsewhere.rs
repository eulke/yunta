//! A run whose process asks a question at its own terminal, through `State` alone, in what
//! its frames cannot hold: its pane says what is asked, the process that asks and since
//! when, and that nothing here answers it; and no key here answers it, the run's log
//! standing as it stood after each.

use surco::doc::Block;
use surco::look::Look;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;

use super::{pane_drawn, scene_of};

/// The target whose cut asks at the terminal of the engine that drives the run.
const TARGET: &str = "asked-elsewhere";

/// The rows `screen`'s palette lists, opened over it, by what each does.
fn palette_rows(screen: &RunScreen) -> Vec<String> {
    let mut opened = screen.state().clone();
    opened.update(yunta_ui::Input::Stroke(Stroke::Insert('/')));
    let overlay = opened.screen(&Look::plain()).overlay;
    let overlay = overlay.expect("the palette stands over the run");
    let listed = overlay.doc.blocks().iter().find_map(|block| match block {
        Block::ChoiceList(list) => list.choices.as_ref(),
        _ => None,
    });
    let listed = listed.expect("the palette lists what can be done here");
    listed.rows.iter().map(|row| row.name.clone()).collect()
}

/// Its pane, under the title of the node asked about: what is asked, the process that asks
/// it, by the id the run's registry names, and since when, to the minute; then that nothing
/// here answers it, and where it is answered.
#[tokio::test]
async fn a_run_asked_elsewhere_says_who_asks_since_when_and_that_nothing_here_answers_it() {
    let screen = run::opened(TARGET).await;
    assert_eq!(scene_of(&screen), Some("asked elsewhere"));
    let pane = pane_drawn(&screen, 84, 26);
    let asks = format!("pid     {}", std::process::id());
    for row in [
        "node `ship` asks at its terminal",
        asks.as_str(),
        "since   00:00 UTC",
        "nothing here answers it: answer it there",
    ] {
        assert!(
            pane.iter().any(|drawn| drawn == row),
            "no `{row}` in {pane:#?}"
        );
    }
}

/// No key on it answers what it asks: neither `a`, which goes to the run the attention row
/// names, nor `enter`, a number or any row of its palette; the run's log, and every other
/// run's, stands as it stood after each.
#[tokio::test]
async fn no_key_on_a_run_asked_elsewhere_answers_it_and_its_log_stands_as_it_stood() {
    let opened = run::opened(TARGET).await;
    let rows = palette_rows(&opened);
    assert!(!rows.is_empty(), "its palette lists what can be done here");
    let keys = [Stroke::Insert('a'), Stroke::Enter, Stroke::Insert('1')];
    let mut pressed: Vec<(String, Vec<Stroke>)> = keys
        .into_iter()
        .map(|key| (format!("{key:?}"), vec![key]))
        .collect();
    for (at, row) in rows.iter().enumerate() {
        let down = std::iter::repeat_n(Stroke::Down, at);
        let taken = [Stroke::Insert('/')]
            .into_iter()
            .chain(down)
            .chain([Stroke::Enter]);
        pressed.push((format!("the palette's row {row:?}"), taken.collect()));
    }
    for (named, strokes) in pressed {
        let mut screen = run::opened(TARGET).await;
        let before = screen.heads();
        screen.pressed(&strokes).await;
        assert_eq!(screen.heads(), before, "{named} moved a log");
    }
}
