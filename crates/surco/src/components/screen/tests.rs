use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::*;
use crate::components::tests::words;
use crate::components::{Answer, Attention, Binding, Gist, Headline, Noted, Ribbon, Stretch};
use crate::glyphs::{Glyphs, Mark};
use crate::golden::assert_golden;
use crate::line::Line;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::{visible, Description, Html, Markdown, Surface, Terminal};
use crate::words::NOTHING_NEEDS_YOU;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens")
}

/// Plain lines, one to each of `rows`.
fn lines(rows: &[&str]) -> Doc {
    Doc::new().with(
        rows.iter()
            .map(|row| Line::new().plain(*row))
            .collect::<Vec<_>>(),
    )
}

/// What a run's screen is about at its plan: the run's headline, the ribbon
/// of its nodes and its gist.
fn about() -> Doc {
    let nodes = [
        "grill",
        "brief",
        "plan",
        "spec",
        "approve",
        "implement",
        "review",
        "lint",
        "tests",
        "ship",
        "pr",
    ];
    let stretches = nodes.into_iter().enumerate().map(|(at, name)| {
        let standing = match at {
            0 | 1 => Standing::Finished,
            2 => Standing::Running,
            _ => Standing::NeverRan,
        };
        Stretch::new(name, standing)
    });
    let headline = Headline::new(Mark::Running, "fragua (standard)", "running")
        .fact("for 7m34s")
        .edge("Q78GRR");
    Doc::new()
        .with(headline)
        .with(Ribbon::new(stretches))
        .with(Gist {
            said: "nothing needs you: it is moving".to_string(),
            on_one_row: false,
        })
}

/// What the plan node is doing, as its pane holds it.
fn work() -> Doc {
    lines(&[
        "what it is doing, 10 calls",
        "read crates/cli/src/commands/pack.rs",
        "read crates/cli/src/cli/pack.rs",
        "read crates/engine/src/catalog.rs",
        "",
        "what it hands over",
        "tasks, not yet",
    ])
}

/// A run's screen as the person reads it at its plan, help open over it.
fn run_screen() -> Screen {
    run_screen_on(false)
}

/// The run's screen of [`run_screen`], the keys on its nodes beside its pane where they stand
/// `on_the_side`.
fn run_screen_on(on_the_side: bool) -> Screen {
    let rail = lines(&[
        "grill  36s",
        "brief  22s",
        "plan",
        "spec",
        "approve",
        "implement",
        "review",
        "lint",
        "tests",
        "ship",
        "pr",
    ]);
    let help = lines(&[
        "/      anything: only what can be done here",
        "n      start a new run, from anywhere",
        "esc    one step back; what it leaves stays waiting",
        "q      leave: every run keeps going",
    ]);
    let mut screen = Screen::new(Region::new(work()).titled(["plan"]))
        .trail(["yunta", "Q78GRR", "plan"])
        .tally(Standing::NeedsYou, 13)
        .tally(Standing::Stalled, 1)
        .about(about())
        .attention(Attention::key("implement asks at its terminal", "a", "go there").of("EKPBGE"))
        .legend(
            Legend::new()
                .arrows("node")
                .key("s", "stats")
                .key("g", "graph")
                .key("q", "field, it keeps going"),
        )
        .overlay(Overlay::new("keys", help).legend(Legend::new().key("esc", "back")));
    screen.side = Some(Side {
        region: Region::new(rail).titled(["nodes"]),
        at: SideAt::Left,
        focused: on_the_side,
        instead: Vec::new(),
    });
    screen
}

/// The run's screen just after an answer was recorded: the note stands in the attention row's
/// place.
fn noted_screen() -> Screen {
    run_screen().noted(Noted {
        mark: Mark::Done,
        said: "Recorded: approve. On to `pr`.".to_string(),
    })
}

/// The run's screen with nothing open over it: its pane titled with its node's kind, who works
/// it at the far end, and the far keys after the screen's own.
fn far_screen() -> Screen {
    let screen = run_screen().anywhere(Legend::new().key("/", "anything").key("?", "keys"));
    Screen {
        main: Region::new(work())
            .titled(["plan", "prompt"])
            .edge("planner on mock/gpt-6-luna"),
        overlay: None,
        ..screen
    }
}

/// A page and a file say a pane's title with its edge, and the far keys after the screen's,
/// as a terminal draws them at their rows' far ends.
#[test]
fn the_far_keys_and_a_titles_edge_are_said_on_every_surface() {
    let screen = far_screen();
    assert_golden(
        &goldens().join("markdown/screen-far.md"),
        &Markdown.screen(&screen),
    );
    assert_golden(
        &goldens().join("html/screen-far.html"),
        &Html.screen(&screen),
    );
}

/// The run's screen with nothing open over it, saying what its run has spent.
fn metered_screen() -> Screen {
    far_screen().meter(crate::components::meter::tests::waited_on_you())
}

/// A page and a file say what the run a screen is about has spent under its regions, before
/// what needs the person, as a terminal draws it on the row over the attention row's place.
#[test]
fn what_the_run_spent_is_said_under_the_regions_on_every_surface() {
    let screen = metered_screen();
    assert_golden(
        &goldens().join("markdown/screen-meter.md"),
        &Markdown.screen(&screen),
    );
    assert_golden(
        &goldens().join("html/screen-meter.html"),
        &Html.screen(&screen),
    );
}

/// The run's screen over a field whose runs need nothing: its tally counted none, and nothing
/// asks for the person.
fn quiet_screen() -> Screen {
    let screen = run_screen();
    Screen {
        tally: Tally::Counted(Vec::new()),
        attention: None,
        overlay: None,
        ..screen
    }
}

/// A page and a file say that nothing needs the person where the screen counted its runs and
/// found none needing a look, as a terminal draws it at the trail's far end.
#[test]
fn a_tally_with_nothing_in_it_is_said_on_every_surface() {
    let screen = quiet_screen();
    assert_golden(
        &goldens().join("markdown/screen-nothing-needs-you.md"),
        &Markdown.screen(&screen),
    );
    assert_golden(
        &goldens().join("html/screen-nothing-needs-you.html"),
        &Html.screen(&screen),
    );
}

/// Where a note stands, a page and a file say it in the attention row's place, as a terminal
/// draws it there.
#[test]
fn a_note_stands_in_the_attention_rows_place_on_every_surface() {
    let screen = noted_screen();
    assert_golden(
        &goldens().join("markdown/screen-noted.md"),
        &Markdown.screen(&screen),
    );
    assert_golden(
        &goldens().join("html/screen-noted.html"),
        &Html.screen(&screen),
    );
}

#[test]
fn a_run_screen_matches_its_goldens() {
    let screen = run_screen();
    assert_golden(
        &goldens().join("markdown/screen.md"),
        &Markdown.screen(&screen),
    );
    assert_golden(&goldens().join("html/screen.html"), &Html.screen(&screen));
}

/// What a key says beside what it does, as a person reads it.
fn said(binding: &Binding) -> String {
    format!("{} {}", binding.keys.spelled(Glyphs::Unicode), binding.does)
}

/// Every word a screen holds, read from its data: each document's as a
/// terminal draws it, and every other string as it stands.
fn held(screen: &Screen) -> BTreeSet<String> {
    let terminal = Terminal::on(Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(120), None),
        motion: crate::motion::Motion::Moves,
    });
    let mut strings = screen.trail.clone();
    strings.extend(match &screen.tally {
        Tally::Uncounted => Vec::new(),
        Tally::Counted(counts) if counts.is_empty() => vec![NOTHING_NEEDS_YOU.to_string()],
        Tally::Counted(counts) => counts.iter().map(Count::said).collect(),
    });
    strings.extend(screen.about.iter().map(|doc| terminal.draw(doc)));
    let side = screen.side.as_ref().map(|side| &side.region);
    for region in std::iter::once(&screen.main).chain(side) {
        strings.extend(region.title.clone());
        strings.extend(region.edge.clone());
        strings.push(terminal.draw(&region.doc));
    }
    if let Some(noted) = &screen.noted {
        strings.push(noted.said.clone());
    } else if let Some(attention) = &screen.attention {
        strings.extend(attention.subject.clone());
        strings.push(attention.asks.clone());
        strings.push(match &attention.answer {
            Answer::Command(command) => command.clone(),
            Answer::Key(binding) => said(binding),
            Answer::Elsewhere | Answer::Unkeyed => String::new(),
        });
    }
    let overlay_keys = screen.overlay.as_ref().map(|overlay| &overlay.legend);
    for legend in [&screen.legend, &screen.anywhere]
        .into_iter()
        .chain(overlay_keys)
    {
        strings.extend(legend.0.iter().map(said));
    }
    if let Some(overlay) = &screen.overlay {
        strings.push(overlay.title.clone());
        strings.push(terminal.draw(&overlay.doc));
    }
    strings.iter().flat_map(|string| words(string)).collect()
}

#[test]
fn every_surface_says_every_word_a_screen_holds() {
    for screen in [run_screen(), noted_screen(), far_screen(), quiet_screen()] {
        let held = held(&screen);
        let forms = [
            ("Markdown", Markdown.screen(&screen)),
            ("Description", Description::default().screen(&screen)),
            ("HTML", visible(&Html.screen(&screen))),
        ];
        for (form, drawn) in &forms {
            let said = words(drawn);
            let lost: Vec<&String> = held.difference(&said).collect();
            assert!(lost.is_empty(), "{form} dropped {lost:?}");
        }
    }
}

/// A page says which region the keys act on, the main one unless the side
/// one has them.
#[test]
fn a_page_marks_the_region_the_keys_act_on() {
    let current = |page: &str, region: &str| {
        page.lines().any(|line| {
            line.starts_with(&format!(r#"<section class="region {region}""#))
                && line.contains(r#"aria-current="true""#)
        })
    };

    let on_main = Html.screen(&run_screen());
    let on_side = Html.screen(&run_screen_on(true));

    assert!(current(&on_main, "main") && !current(&on_main, "side"));
    assert!(current(&on_side, "side") && !current(&on_side, "main"));
}

/// A page a screen stands on keeps every block but the faint line a page about no one run
/// stands under, which the screen's rule stands for; a furrow of a run's nodes stays.
#[test]
fn a_page_on_a_screen_leaves_its_unplowed_line_to_the_screens_rule() {
    let headline = || Headline::new(Mark::Done, "pack acme/demo", "installed");
    let page = Doc::new()
        .with(headline())
        .with(Furrow(Vec::new()))
        .with(vec![Line::new().plain("at  .yunta/packs/acme/demo")]);
    let region = Region::of_a_page(page);
    let kinds: Vec<bool> = region
        .doc
        .blocks()
        .iter()
        .map(|block| matches!(block, Block::Furrow(_)))
        .collect();
    assert_eq!(kinds, [false, false], "the headline and its line stay");
    let run = Doc::new()
        .with(headline())
        .with(Furrow(vec![Standing::Finished, Standing::Running]));
    let furrowed = Region::of_a_page(run).doc.blocks().len();
    assert_eq!(furrowed, 2, "a run's furrow stays");
}

/// A page's sentence under its headline stands on a screen as a gist does: on its one row,
/// one step in, with no closing period, cut at the pane's edge where it does not fit, where
/// the printed page wraps it whole and ends it with a period.
#[test]
fn a_pages_sentence_on_a_screen_stands_on_its_row_as_a_gist() {
    let sentence = "no run is collected: no retention is configured to guess from";
    let page = || {
        Doc::new()
            .with(Headline::new(
                Mark::Absent,
                "upkeep",
                "would reclaim no run",
            ))
            .with(Gist::new(sentence))
    };
    let drawn = |cells: usize| {
        let look = Look {
            glyphs: Glyphs::Unicode,
            paint: Paint::Plain,
            width: Width::exact(cells),
            motion: crate::motion::Motion::Still,
        };
        let lines = Terminal::on(look).pane(&Region::of_a_page(page()), 4).lines;
        lines.iter().map(Line::text).collect::<Vec<_>>()
    };
    assert_eq!(
        drawn(80)[1],
        format!("  {sentence}"),
        "whole, with no period"
    );
    assert_eq!(
        drawn(40)[1],
        "  no run is collected: no retention is…",
        "cut at the edge, on its one row"
    );
    assert_eq!(
        drawn(40).len(),
        2,
        "nothing of it wraps onto a row under it"
    );
}
