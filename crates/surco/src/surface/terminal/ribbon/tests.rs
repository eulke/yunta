use super::*;
use crate::components::Furrow;
use crate::doc::Block;
use crate::glyphs::Glyphs;
use crate::look::Width;
use crate::paint::Paint;
use crate::surface::Terminal;

/// A plain look `cells` wide, in Unicode.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(cells), None),
    }
}

fn run() -> Ribbon {
    Ribbon::new([
        Stretch::new("grill", Standing::Finished),
        Stretch::new("plan", Standing::Running),
        Stretch::new("ship", Standing::NeverRan),
    ])
}

/// A ribbon with no events has no plow: no cell of its strokes is strong.
#[test]
fn a_ribbon_with_no_events_draws_no_plow() {
    let lines = run().lines(&look(80));
    let strong = lines[1]
        .spans()
        .iter()
        .any(|span| span.tone == Tone::Strong);
    assert!(!strong, "{:?}", lines[1]);
}

/// The plow stands a cell further along the running node's stretch for each
/// event, and starts over at the stretch's end.
#[test]
fn the_plow_stands_where_the_running_nodes_events_put_it() {
    let strokes = |events| {
        let lines = run().plowed(events).lines(&look(80));
        let at = lines[1]
            .spans()
            .iter()
            .take_while(|span| span.tone != Tone::Strong)
            .map(|span| cell_width(&span.text))
            .sum::<usize>();
        at
    };
    // The running stretch starts after `grill` (five cells) and a gap.
    assert_eq!(strokes(0), 6);
    assert_eq!(strokes(3), 9);
    assert_eq!(strokes(4), 6);
}

/// A ribbon with no stretch has nothing to say, and draws nothing.
#[test]
fn a_ribbon_with_no_stretch_draws_nothing() {
    assert!(Ribbon::new([]).lines(&look(80)).is_empty());
}

/// Where the names do not fit, the ribbon is the page's furrow, drawn as the
/// furrow draws it.
#[test]
fn a_ribbon_whose_names_do_not_fit_is_the_page_furrow() {
    let names = [
        "implement",
        "review",
        "conform",
        "approve",
        "fix-lint",
        "tests",
        "ship",
    ];
    let standings = [Standing::Finished, Standing::Running, Standing::NeverRan];
    let stretches = names
        .iter()
        .enumerate()
        .map(|(at, name)| Stretch::new(format!("{name}-{at}"), standings[at % 3]));
    let ribbon = Ribbon::new(stretches);
    let furrow = Furrow(
        names
            .iter()
            .enumerate()
            .map(|(at, _)| standings[at % 3])
            .collect(),
    );
    let terminal = Terminal::on(look(60));

    let drawn = terminal.block(&Block::Ribbon(ribbon));

    assert_eq!(drawn, terminal.block(&Block::Furrow(furrow)));
}
