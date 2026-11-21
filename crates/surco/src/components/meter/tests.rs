//! A meter is a block every surface draws: as Markdown, as a page, and on a terminal in every
//! environment, on one row that lets its parts go whole where it passes the edge.

use std::path::{Path, PathBuf};

use super::{Meter, Time};
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/meter")
}

/// A run of fifteen nodes, the first three of which spent tokens, that has been at work two
/// minutes and a half and waited five minutes on the person.
pub(crate) fn waited_on_you() -> Meter {
    let mut bars = vec![220_000.0, 112_000.0, 635_000.0];
    bars.resize(15, 0.0);
    Meter {
        count: "967k tokens".to_string(),
        bars,
        times: vec![
            Time {
                figure: "2m34s".to_string(),
                label: "at work".to_string(),
                by_person: false,
            },
            Time {
                figure: "5m00s".to_string(),
                label: "waiting on you".to_string(),
                by_person: true,
            },
        ],
    }
}

/// A run of one node that has spent nothing, at work three quarters of a minute.
fn spent_nothing() -> Meter {
    Meter {
        count: "0 tokens".to_string(),
        bars: vec![0.0],
        times: vec![Time {
            figure: "45s".to_string(),
            label: "at work".to_string(),
            by_person: false,
        }],
    }
}

/// `meter` drawn as Markdown, as a page, and on a terminal in every environment, each
/// against its golden named `name`.
fn matches_its_goldens(meter: Meter, name: &str) {
    let doc = Doc::new().with(meter);
    assert_golden(&goldens().join(format!("{name}.md")), &Markdown.draw(&doc));
    assert_golden(&goldens().join(format!("{name}.html")), &Html.body(&doc));
    for environment in &ENVIRONMENTS {
        let drawn = Terminal::on(Look::of(environment)).draw(&doc);
        assert_golden(&environment.golden(&goldens(), name), &drawn);
    }
}

#[test]
fn a_meter_matches_its_goldens_on_every_surface() {
    matches_its_goldens(waited_on_you(), "waited-on-you");
}

/// A node that spent nothing is the lowest bar, also where no node spent anything: the
/// count beside the bars says there is nothing to scale them against.
#[test]
fn a_meter_of_nothing_spent_draws_the_lowest_bar_a_node() {
    matches_its_goldens(spent_nothing(), "spent-nothing");
}

/// The row stands whole where it fits. Where it passes the edge its bars go first, then each
/// time from the last, every part whole, so the count and every figure left stand whole.
#[test]
fn a_narrow_row_lets_the_bars_go_then_the_last_time_and_keeps_its_count_whole() {
    for (edge, drawn) in [
        (
            120,
            "967k tokens ▃▂█▁▁▁▁▁▁▁▁▁▁▁▁ · 2m34s at work · 5m00s waiting on you",
        ),
        (60, "967k tokens · 2m34s at work · 5m00s waiting on you"),
        (49, "967k tokens · 2m34s at work"),
        (20, "967k tokens"),
    ] {
        let look = Look {
            glyphs: Glyphs::Unicode,
            paint: Paint::Plain,
            width: Width::exact(edge),
            motion: Motion::Moves,
        };
        let row = Terminal::on(look).meter(&waited_on_you());
        assert_eq!(row.text(), drawn, "at {edge} cells");
    }
}

/// A screen short of rows draws the meter in figures alone: its bars go where they would fit,
/// and its times go from the last as anywhere else, every part whole.
#[test]
fn in_figures_alone_the_bars_go_where_they_would_fit() {
    for (edge, meter, drawn) in [
        (58, spent_nothing(), "0 tokens · 45s at work"),
        (
            78,
            waited_on_you(),
            "967k tokens · 2m34s at work · 5m00s waiting on you",
        ),
        (49, waited_on_you(), "967k tokens · 2m34s at work"),
    ] {
        let look = Look {
            glyphs: Glyphs::Unicode,
            paint: Paint::Plain,
            width: Width::exact(edge),
            motion: Motion::Moves,
        };
        let terminal = Terminal::on(look);
        assert_eq!(
            terminal.meter_figures(&meter).text(),
            drawn,
            "at {edge} cells"
        );
    }
}
