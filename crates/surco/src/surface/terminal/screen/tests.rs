//! A screen's rows that are no document: the trail with the tally kept whole at its end,
//! the rule across the width, and a title strong or muted by where the keys act.

use std::path::{Path, PathBuf};

use crate::components::{Attention, Count, Legend, Noted, Region, Tally};
use crate::doc::Doc;
use crate::glyphs::Mark;
use crate::golden::{assert_golden, Environment, ENVIRONMENTS};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::Terminal;
use crate::width::cell_width;
use crate::words::Standing;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/screen")
}

/// A terminal whose lines are exactly `cells` wide, in `environment`'s glyphs and paint.
fn on(environment: &Environment, cells: usize) -> Terminal {
    Terminal::on(Look {
        width: Width::exact(cells),
        ..Look::of(environment)
    })
}

fn places(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// Thirteen runs that need the person and one stalled.
fn tally() -> Tally {
    Tally::Counted(vec![
        Count {
            standing: Standing::NeedsYou,
            runs: 13,
        },
        Count {
            standing: Standing::Stalled,
            runs: 1,
        },
    ])
}

/// The reference's trail row, inside its margins: 78 cells in ASCII, 58 in Unicode.
#[test]
fn the_trail_stands_at_the_start_and_the_tally_at_the_end() {
    let trail = places(&["yunta", "Q78GRR", "plan"]);
    let ascii = on(&ENVIRONMENTS[1], 78).trail(&trail, &tally());
    assert_eq!(
        ascii.text(),
        "yunta :: Q78GRR :: plan                            ? 13 need you ; ^ 1 stalled"
    );
    let unicode = on(&ENVIRONMENTS[2], 58).trail(&trail, &tally());
    assert_eq!(
        unicode.text(),
        "yunta › Q78GRR › plan          ◆ 13 need you · ▲ 1 stalled"
    );
}

/// A row too short for the trail and the whole tally keeps the trail whole and lets the
/// tally's words go, its marks and counts staying, as the target `findings` draws its first
/// row at 60 columns.
#[test]
fn a_short_row_lets_the_tallys_words_go_before_the_trail_is_cut() {
    let one_each = Tally::Counted(vec![
        Count {
            standing: Standing::NeedsYou,
            runs: 1,
        },
        Count {
            standing: Standing::Stalled,
            runs: 1,
        },
    ]);
    let trail = places(&["yunta", "Q78GRR", "review@reviewer"]);
    let row = on(&ENVIRONMENTS[2], 58).trail(&trail, &one_each);
    assert_eq!(
        row.text(),
        "yunta › Q78GRR › review@reviewer                 ◆ 1 · ▲ 1"
    );
}

/// A row too short for the trail even beside the tally's marks and counts cuts the trail and
/// keeps them whole.
#[test]
fn a_row_too_short_for_the_marks_and_counts_cuts_the_trail() {
    let trail = places(&[
        "yunta",
        "Q78GRR",
        "implement",
        "the third task",
        "its proof",
    ]);
    let row = on(&ENVIRONMENTS[2], 58).trail(&trail, &tally());
    let text = row.text();
    assert_eq!(cell_width(&text), 58, "{text}");
    assert!(text.starts_with("yunta › Q78GRR"), "{text}");
    assert!(text.contains('…'), "{text}");
    assert!(text.ends_with("◆ 13 · ▲ 1"), "{text}");
}

/// On a screen that counts nothing, the trail stands alone.
#[test]
fn with_nothing_counted_the_trail_stands_alone() {
    let row = on(&ENVIRONMENTS[2], 58).trail(&places(&["yunta"]), &Tally::Uncounted);
    assert_eq!(row.text(), "yunta");
}

/// A screen that counted its runs and found none needing a look says so at the trail's far
/// end, muted; one that counts nothing says nothing there.
#[test]
fn a_counted_empty_tally_says_nothing_needs_you_and_an_uncounted_one_nothing() {
    let trail = places(&["yunta", "/home/set-up"]);
    let terminal = on(&ENVIRONMENTS[2], 58);
    let counted = terminal.trail(&trail, &Tally::Counted(Vec::new()));
    assert_eq!(
        counted.text(),
        format!("yunta › /home/set-up{}nothing needs you", " ".repeat(21))
    );
    assert_eq!(
        counted.spans().last().map(|span| span.tone),
        Some(Tone::Muted)
    );
    let uncounted = terminal.trail(&trail, &Tally::Uncounted);
    assert_eq!(uncounted.text(), "yunta › /home/set-up");
}

/// The trail with each tally on terminals of 80 and 60 columns, inside their margins, in every
/// environment: a first request's and the workflows' over a field whose runs need nothing, the
/// signpost's, which follows nothing, and a field's whose runs need the person.
#[test]
fn the_trails_with_each_tally_match_their_goldens_in_every_environment() {
    let needing = || {
        Tally::Counted(vec![
            Count {
                standing: Standing::NeedsYou,
                runs: 5,
            },
            Count {
                standing: Standing::Stalled,
                runs: 1,
            },
        ])
    };
    let trails = || {
        [
            (
                places(&["yunta", "/home/set-up"]),
                Tally::Counted(Vec::new()),
            ),
            (places(&["yunta", "workflows"]), Tally::Counted(Vec::new())),
            (places(&["yunta"]), Tally::Uncounted),
            (places(&["yunta", "/cut/worktree"]), needing()),
        ]
    };
    for environment in &ENVIRONMENTS {
        let rows = [78, 58].into_iter().flat_map(|cells| {
            trails().map(|(trail, tally)| on(environment, cells).trail(&trail, &tally))
        });
        assert_golden(
            &environment.golden(&goldens(), "tally"),
            &painted(environment, rows),
        );
    }
}

/// The rule is the furrow of no node: every cell the ground not yet plowed.
#[test]
fn the_rule_takes_every_cell_with_the_ground_not_yet_plowed() {
    assert_eq!(on(&ENVIRONMENTS[2], 60).rule().text(), "┄".repeat(60));
    assert_eq!(on(&ENVIRONMENTS[1], 80).rule().text(), ".".repeat(80));
}

fn tones(line: &Line) -> Vec<Tone> {
    line.spans().iter().map(|span| span.tone).collect()
}

/// The edge between two regions is an upright with a cell either side, faint.
#[test]
fn the_edge_is_an_upright_between_two_cells() {
    let unicode = on(&ENVIRONMENTS[2], 60).edge();
    assert_eq!(unicode.text(), " │ ");
    assert_eq!(tones(&unicode), [Tone::Faint]);
    assert_eq!(on(&ENVIRONMENTS[1], 80).edge().text(), " | ");
}

/// A region titled `parts`, holding nothing.
fn titled(parts: &[&str]) -> Region {
    Region::new(Doc::new()).titled(parts.iter().copied())
}

/// A title is strong where the keys act on its region, and muted elsewhere.
#[test]
fn a_title_is_strong_where_the_keys_act_and_muted_elsewhere() {
    let terminal = on(&ENVIRONMENTS[2], 22);
    let nodes = titled(&["nodes"]);
    assert_eq!(terminal.title(&nodes, true).text(), "nodes");
    assert_eq!(tones(&terminal.title(&nodes, true)), [Tone::Strong]);
    assert_eq!(tones(&terminal.title(&nodes, false)), [Tone::Muted]);
}

/// A title wider than its region is cut.
#[test]
fn a_title_wider_than_its_region_is_cut() {
    let title = on(&ENVIRONMENTS[2], 10).title(&titled(&["what the run hands over"]), true);
    assert_eq!(title.text(), "what the…");
}

/// A title's parts stand as a desk's do, the first in the title's tone and the rest muted after
/// the separator, and its edge stands at the row's far end, muted.
#[test]
fn a_title_stands_in_its_parts_and_its_edge_at_the_far_end() {
    let pane = titled(&["plan", "prompt"]).edge("planner on mock/gpt-6-luna");
    let row = on(&ENVIRONMENTS[2], 58).title(&pane, true);
    assert_eq!(
        row.text(),
        format!("plan · prompt{}planner on mock/gpt-6-luna", " ".repeat(19))
    );
    assert_eq!(
        tones(&row),
        [
            Tone::Strong,
            Tone::Muted,
            Tone::Muted,
            Tone::Plain,
            Tone::Muted
        ]
    );
}

/// A title with an edge on a terminal of 60 columns, its region's 58 cells, too narrow for
/// both: the title gives up its later parts whole, then is cut, and the edge stands whole.
#[test]
fn a_title_with_an_edge_is_cut_and_the_edge_kept() {
    let edge = "executor on claude-code/claude-opus-latest";
    let terminal = on(&ENVIRONMENTS[2], 58);
    let parted = terminal.title(&titled(&["fix-findings", "prompt"]).edge(edge), true);
    assert_eq!(parted.text(), format!("fix-findings    {edge}"));
    let cut = terminal.title(&titled(&["review@reviewer-alt", "prompt"]).edge(edge), true);
    assert_eq!(cut.text(), format!("review@review…  {edge}"));
    assert_eq!(cell_width(&cut.text()), 58);
}

/// A note on a terminal of 60 columns, inside its margins, is one row: its done mark first,
/// in its tone, then its sentence plain, cut at the row's end with the ellipsis, and nothing
/// tints it.
#[test]
fn a_note_is_one_row_its_mark_first_and_its_sentence_cut() {
    let noted = Noted {
        mark: Mark::Done,
        said: "Nothing to resume: this one waits on a decision. Enter reads it.".to_string(),
    };
    let row = on(&ENVIRONMENTS[2], 58).noted(&noted);
    assert_eq!(
        row.text(),
        "✓ Nothing to resume: this one waits on a decision. Enter…"
    );
    assert_eq!(row.spans()[0].text, "✓");
    assert_eq!(tones(&row)[0], Tone::Done);
    assert!(tones(&row)[1..].iter().all(|tone| *tone == Tone::Plain));
    assert_eq!(row.fill(), None);
}

/// What four acts came to, each said once until the next key: an answer recorded, a resume
/// refused, a round put aside and a run resumed.
fn notes() -> Vec<Noted> {
    [
        "Recorded: approve. On to `pr`.",
        "Nothing to resume: this one waits on a decision. Enter reads it.",
        "Nothing recorded. The run stays waiting on these questions.",
        "C0NS0E resumed, driving forward independently.",
    ]
    .into_iter()
    .map(|said| Noted {
        mark: Mark::Done,
        said: said.to_string(),
    })
    .collect()
}

/// The notes on terminals of 80 and 60 columns, inside their margins, in every environment.
#[test]
fn the_notes_match_their_goldens_in_every_environment() {
    for environment in &ENVIRONMENTS {
        let terminal = on(environment, environment.width);
        let drawn: String = [78, 58]
            .into_iter()
            .flat_map(|cells| notes().into_iter().map(move |noted| (cells, noted)))
            .map(|(cells, noted)| on(environment, cells).noted(&noted))
            .map(|line| format!("{}\n", terminal.look.paint.line(&line)))
            .collect();
        assert_golden(&environment.golden(&goldens(), "noted"), &drawn);
    }
}

/// The trail, the rule, two titles, the attention and the legend, at the reference's
/// widths and in every environment.
#[test]
fn the_rows_match_their_goldens_in_every_environment() {
    let attention = Attention::key("implement asks at its terminal", "a", "go there").of("EKPBGE");
    let legend = Legend::new()
        .arrows("node")
        .key("s", "stats")
        .key("g", "graph")
        .key("q", "field, it keeps going");
    for environment in &ENVIRONMENTS {
        let inner = environment.width - 2;
        let rows = [
            on(environment, inner).trail(&places(&["yunta", "Q78GRR", "plan"]), &tally()),
            on(environment, environment.width).rule(),
            on(environment, 22).title(&titled(&["nodes"]), false),
            on(environment, environment.width).edge(),
            on(environment, inner).title(&titled(&["plan"]), true),
            on(environment, inner).attention(&attention),
            on(environment, inner).legend(&legend, &Legend::new()),
        ];
        let terminal = on(environment, environment.width);
        let drawn: String = rows
            .iter()
            .map(|line| format!("{}\n", terminal.look.paint.line(line)))
            .collect();
        assert_golden(&environment.golden(&goldens(), "rows"), &drawn);
    }
}

/// `lines` as `environment` paints them, a line a row.
fn painted(environment: &Environment, lines: impl IntoIterator<Item = Line>) -> String {
    let terminal = on(environment, environment.width);
    lines
        .into_iter()
        .map(|line| format!("{}\n", terminal.look.paint.line(&line)))
        .collect()
}

/// A run's keys while it works and a plan's while it is decided on, with the far keys, on
/// terminals of 120, 80 and 60 columns inside their margins, in every environment.
#[test]
fn the_legends_with_their_far_keys_match_their_goldens_in_every_environment() {
    let anywhere = Legend::new().key("/", "anything").key("?", "keys");
    let at_work = || {
        Legend::new()
            .key("j k", "node")
            .key("s", "stats")
            .key("g", "graph")
            .key("q", "field, it keeps going")
    };
    let deciding = || {
        Legend::new()
            .key("1 2 3", "answer")
            .key("tab", "next place")
            .arrows("scroll")
            .key("p", "proof")
            .key("d", "decided for you")
            .key("esc", "leave it waiting")
    };
    for environment in &ENVIRONMENTS {
        let rows = [118, 78, 58].into_iter().flat_map(|cells| {
            [at_work(), deciding()].map(|legend| on(environment, cells).legend(&legend, &anywhere))
        });
        assert_golden(
            &environment.golden(&goldens(), "legend"),
            &painted(environment, rows),
        );
    }
}

/// A pane's titles as a run's screen shows them, each with who works its node, its check or
/// its assignee at the far end, one where the keys act elsewhere and one with nothing at its far
/// end: in a pane of 84 cells beside the nodes, and of 78 and 58 alone, in every environment.
#[test]
fn the_titles_with_their_edges_match_their_goldens_in_every_environment() {
    let titles = [
        (
            &["plan", "prompt"],
            Some("planner on mock/gpt-6-luna"),
            true,
        ),
        (
            &["implement", "loop"],
            Some("executor on mock/gpt-6-luna"),
            false,
        ),
        (&["tests", "check"], Some("baseline_compare"), true),
        (&["ship", "gate"], Some("assignee lead"), true),
        (&["build", "bash"], None, true),
    ];
    for environment in &ENVIRONMENTS {
        let rows = [84, 78, 58].into_iter().flat_map(|cells| {
            titles.map(|(parts, edge, focused)| {
                let region = edge
                    .into_iter()
                    .fold(titled(parts), |region, edge| region.edge(edge));
                on(environment, cells).title(&region, focused)
            })
        });
        assert_golden(
            &environment.golden(&goldens(), "title"),
            &painted(environment, rows),
        );
    }
}

/// What a run's screen is about: its headline, a ribbon of three nodes whose names fit,
/// and its gist on a row of its own.
fn about() -> Doc {
    let stretches = [
        ("start", Standing::Finished),
        ("extra", Standing::Skipped),
        ("ship", Standing::Finished),
    ]
    .map(|(name, standing)| crate::components::Stretch::new(name, standing));
    Doc::new()
        .with(crate::components::Headline::new(
            Mark::Done,
            "mode-scenario (alpha)",
            "finished",
        ))
        .with(crate::components::Ribbon::new(stretches))
        .with(crate::doc::Block::Rows(vec![
            Line::new().plain("the run's gist, one row however long it says it")
        ]))
}

/// What a screen is about stands as a band, no blank row between its blocks; short of
/// rows, the ribbon's names give way first, its strokes alone keeping their row, and then
/// the band is cut from its last row.
#[test]
fn what_a_screen_is_about_gives_way_by_the_ribbons_names_first() {
    let terminal = on(&ENVIRONMENTS[1], 58);
    let text = |rows: usize| -> Vec<String> {
        let band = terminal.across(&about(), rows);
        band.iter()
            .map(|line| line.text().trim_end().to_string())
            .collect()
    };
    let whole = text(usize::MAX);
    assert_eq!(whole.len(), 4, "{whole:#?}");
    assert_eq!(whole[1], "start extra ship");
    assert!(whole.iter().all(|row| !row.is_empty()), "{whole:#?}");
    let short = text(3);
    assert_eq!(short[0], whole[0]);
    assert!(
        short[1].chars().count() > 50,
        "the strokes share the row: {short:#?}"
    );
    assert_eq!(short[2], whole[3]);
    assert_eq!(text(2), short[..2]);
}
