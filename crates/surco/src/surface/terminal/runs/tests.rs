use crate::components::{Edge, RunLine, Runs};
use crate::glyphs::Glyphs;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::words::Standing;

/// A run's row, pointed at or picked as `pointed` and `marked` say.
fn row(handle: &str, pointed: bool, marked: bool) -> RunLine {
    RunLine {
        standing: Standing::Stopped,
        handle: handle.to_string(),
        workflow: "fragua".to_string(),
        mode: "quick".to_string(),
        furrow: vec![Standing::Finished, Standing::Stopped],
        said: "stopped while it ran".to_string(),
        edge: Edge::Age("3m".to_string()),
        pointed,
        marked,
    }
}

fn drawn(glyphs: Glyphs) -> Vec<String> {
    let runs = Runs {
        rows: vec![
            row("AAAAAA", true, false),
            row("BBBBBB", false, true),
            row("CCCCCC", true, true),
            row("DDDDDD", false, false),
        ],
    };
    let look = Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::exact(80),
        motion: crate::motion::Motion::Moves,
    };
    runs.lines(&look).iter().map(|line| line.text()).collect()
}

/// The row a key acts on carries the pointer, and a run picked for an act that takes
/// several carries the picked mark, in the two cells before where it stands; a row
/// neither is keeps them blank.
#[test]
fn a_pointed_row_and_a_picked_one_say_so_before_where_they_stand() {
    for (glyphs, pointer, picked) in [(Glyphs::Unicode, '▸', '■'), (Glyphs::Ascii, '*', '+')] {
        let rows = drawn(glyphs);
        let mark = glyphs.mark(Standing::Stopped.mark());
        assert!(
            rows[0].starts_with(&format!("{pointer} {mark} AAAAAA")),
            "{rows:#?}"
        );
        assert!(
            rows[1].starts_with(&format!(" {picked}{mark} BBBBBB")),
            "{rows:#?}"
        );
        assert!(
            rows[2].starts_with(&format!("{pointer}{picked}{mark} CCCCCC")),
            "{rows:#?}"
        );
        assert!(
            rows[3].starts_with(&format!("  {mark} DDDDDD")),
            "{rows:#?}"
        );
    }
}
