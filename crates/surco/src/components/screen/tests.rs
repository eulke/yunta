use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::*;
use crate::components::tests::words;
use crate::components::{Answer, Attention, Binding, Gist, Headline, Ribbon, Stretch};
use crate::glyphs::{Glyphs, Mark};
use crate::golden::assert_golden;
use crate::line::Line;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::{visible, Description, Html, Markdown, Surface, Terminal};

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
        .with(Gist("nothing needs you: it is moving".to_string()))
}

/// A run's screen as the person reads it at its plan, help open over it.
fn run_screen() -> Screen {
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
    let work = lines(&[
        "what it is doing, 10 calls",
        "read crates/cli/src/commands/pack.rs",
        "read crates/cli/src/cli/pack.rs",
        "read crates/engine/src/catalog.rs",
        "",
        "what it hands over",
        "tasks, not yet",
    ]);
    let help = lines(&[
        "/      anything: only what can be done here",
        "n      start a new run, from anywhere",
        "esc    one step back; what it leaves stays waiting",
        "q      leave: every run keeps going",
    ]);
    Screen::new(Region::new(work).titled("plan"))
        .trail(["yunta", "Q78GRR", "plan"])
        .tally(Standing::NeedsYou, 13)
        .tally(Standing::Stalled, 1)
        .about(about())
        .side(Region::new(rail).titled("nodes"))
        .attention(Attention::key("implement asks at its terminal", "a", "go there").of("EKPBGE"))
        .legend(
            Legend::new()
                .arrows("node")
                .key("s", "stats")
                .key("g", "graph")
                .key("q", "field, it keeps going"),
        )
        .overlay(Overlay::new("keys", help).legend(Legend::new().key("esc", "back")))
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
    strings.extend(screen.tally.iter().map(Count::said));
    strings.extend(screen.about.iter().map(|doc| terminal.draw(doc)));
    let side = screen.side.as_ref().map(|side| &side.region);
    for region in std::iter::once(&screen.main).chain(side) {
        strings.extend(region.title.clone());
        strings.push(terminal.draw(&region.doc));
    }
    if let Some(attention) = &screen.attention {
        strings.extend(attention.subject.clone());
        strings.push(attention.asks.clone());
        strings.push(match &attention.answer {
            Answer::Command(command) => command.clone(),
            Answer::Key(binding) => said(binding),
            Answer::Elsewhere => String::new(),
        });
    }
    let overlay_keys = screen.overlay.as_ref().map(|overlay| &overlay.legend);
    for legend in std::iter::once(&screen.legend).chain(overlay_keys) {
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
    let screen = run_screen();
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
    let on_side = Html.screen(&run_screen().focus_side());

    assert!(current(&on_main, "main") && !current(&on_main, "side"));
    assert!(current(&on_side, "side") && !current(&on_side, "main"));
}
