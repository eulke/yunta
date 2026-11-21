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
        motion: crate::motion::Motion::Moves,
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

/// Where the names do not fit, the ribbon draws its strokes alone, a stroke a node, each
/// as long as the line shares out evenly less the gap after it: fifteen nodes on 78 cells
/// take four cells each, and the cells left over stay blank at the line's end.
#[test]
fn a_ribbon_whose_names_do_not_fit_draws_even_strokes_alone() {
    let standings = [Standing::Finished, Standing::Running, Standing::NeverRan];
    let stretches = (0..15).map(|at| Stretch::new(format!("implement-{at}"), standings[at.min(2)]));
    let drawn = Terminal::on(look(78)).block(&Block::Ribbon(Ribbon::new(stretches)));
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    let strokes: Vec<String> = drawn[0].text().split(' ').map(String::from).collect();
    assert_eq!(strokes.len(), 15, "{strokes:?}");
    assert!(
        strokes.iter().all(|stroke| cell_width(stroke) == 4),
        "{strokes:?}"
    );
}

/// A line too narrow for a cell and a gap a node draws the page's furrow, a cell standing
/// for the nodes it covers.
#[test]
fn a_ribbon_too_many_for_its_line_is_the_page_furrow() {
    let standings = [Standing::Finished, Standing::Running, Standing::NeverRan];
    let stretches = (0..40).map(|at| Stretch::new(format!("n{at}"), standings[at % 3]));
    let furrow = Furrow((0..40).map(|at| standings[at % 3]).collect());
    let terminal = Terminal::on(look(60));
    assert_eq!(
        terminal.block(&Block::Ribbon(Ribbon::new(stretches))),
        terminal.block(&Block::Furrow(furrow))
    );
}
