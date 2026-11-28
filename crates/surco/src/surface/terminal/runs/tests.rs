use crate::components::{Edge, RunLine, Runs};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::line::Tone;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::Terminal;
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
    laid(&runs, look).iter().map(|line| line.text()).collect()
}

/// `runs` as a terminal of `look` lays the block out.
fn laid(runs: &Runs, look: Look) -> Vec<crate::line::Line> {
    Terminal::on(look).block(&Block::Runs(runs.clone()))
}

/// The row a key acts on carries the pointer, and a run picked for an act that takes
/// several carries the picked mark, in the two cells before where it stands; a row
/// neither is keeps them blank.
#[test]
fn a_pointed_row_and_a_picked_one_say_so_before_where_they_stand() {
    for (glyphs, pointer, picked) in [(Glyphs::Unicode, '▸', '◉'), (Glyphs::Ascii, '*', '+')] {
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

/// A run of the field, `standing`, its nodes standing as `furrow` says, holding as `said`
/// and standing so for `age`.
fn run(
    handle: &str,
    mode: &str,
    standing: Standing,
    furrow: &[Standing],
    said: &str,
    age: &str,
) -> RunLine {
    RunLine {
        standing,
        handle: handle.to_string(),
        workflow: "fragua".to_string(),
        mode: mode.to_string(),
        furrow: furrow.to_vec(),
        said: said.to_string(),
        edge: Edge::Age(age.to_string()),
        pointed: false,
        marked: false,
    }
}

/// `runs` drawn `cells` wide in Unicode, unpainted, a row a string.
fn drawn_at(runs: &Runs, cells: usize) -> Vec<String> {
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: crate::motion::Motion::Still,
    };
    laid(runs, look).iter().map(|line| line.text()).collect()
}

/// `count` nodes, the first `reached` of them reached, the rest never run.
fn nodes(count: usize, reached: usize) -> Vec<Standing> {
    (0..count)
        .map(|at| match at < reached {
            true => Standing::Finished,
            false => Standing::NeverRan,
        })
        .collect()
}

/// Where the furrow of `row` starts and how many cells it takes: from its first stroke to
/// its last, the gaps between them among them.
fn furrow_at(row: &str) -> (usize, usize) {
    let cells: Vec<char> = row.chars().collect();
    let stroke = |c: &char| matches!(c, '━' | '┄' | '╌');
    let start = cells.iter().position(stroke).expect("a furrow");
    let end = cells.iter().rposition(stroke).expect("a furrow");
    (start, end + 1 - start)
}

/// Two runs of 15 and 4 nodes draw their furrows from one cell, each in its column of
/// sixteen: two cells a node at most, a gap between two strokes where there are two a node,
/// so 15 nodes take all sixteen and 4 take eight. At 60 cells the furrow stands and what
/// holds each run is cut to what is left, with the ellipsis.
#[test]
fn two_runs_draw_their_furrows_from_one_cell_in_a_column_of_sixteen() {
    let said = "node `implement` failed: the suite exits 101 where it passed at baseline";
    let runs = Runs {
        rows: vec![
            run(
                "Q78GRR",
                "standard",
                Standing::NeedsYou,
                &nodes(15, 13),
                said,
                "26m",
            ),
            run(
                "EKPBGE",
                "quick",
                Standing::Stalled,
                &nodes(4, 1),
                said,
                "11d",
            ),
        ],
    };
    for cells in [80, 60] {
        let rows = drawn_at(&runs, cells);
        let (long, short) = (furrow_at(&rows[0]), furrow_at(&rows[1]));
        assert_eq!(long.0, short.0, "{rows:#?}");
        assert_eq!((long.1, short.1), (16, 8), "{rows:#?}");
        for row in &rows {
            assert_eq!(row.chars().count(), cells, "{rows:#?}");
            assert!(row.contains('…'), "{rows:#?}");
        }
    }
    let rows = drawn_at(&runs, 60);
    assert_eq!(
        rows[0],
        "  ◆ Q78GRR standard  ━━━━━━━━━━━━━━┄┄ node `implement`…  26m"
    );
    assert_eq!(
        rows[1],
        "  ▲ EKPBGE quick     ━ ┄┄ ┄ ┄         node `implement`…  11d"
    );
}

/// A run of more nodes than its column has cells keeps to the column, from the same cell as
/// every other run's furrow, and each cell takes the most urgent word of the nodes it covers:
/// the node that needs the person is seen among twenty.
#[test]
fn an_over_full_furrow_keeps_to_its_column_and_shows_what_needs_the_person() {
    let said = "node `review` asks at its terminal";
    let mut twenty = nodes(20, 9);
    twenty[9] = Standing::NeedsYou;
    let runs = Runs {
        rows: vec![
            run(
                "Q78GRR",
                "standard",
                Standing::NeedsYou,
                &nodes(15, 13),
                said,
                "26m",
            ),
            run("T0OMNY", "full", Standing::NeedsYou, &twenty, said, "3m"),
        ],
    };
    let rows = drawn_at(&runs, 80);
    assert_eq!(furrow_at(&rows[0]), furrow_at(&rows[1]), "{rows:#?}");
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(80),
        motion: crate::motion::Motion::Still,
    };
    let needs = Tone::of(Standing::NeedsYou.mark());
    let furrow_tones: Vec<Tone> = laid(&runs, look)[1]
        .spans()
        .iter()
        .filter(|span| span.text.contains('━'))
        .map(|span| span.tone)
        .collect();
    assert!(furrow_tones.contains(&needs), "{furrow_tones:?}");
}

/// Four runs of the field's targets, the first pointed at: the run asking at its terminal,
/// a quick run whose first node failed and which leaves a node out, a run at a gate no forge
/// reaches, and a run a person stopped.
fn the_targets_runs() -> Runs {
    use Standing::{Failed, NeedsYou, NeverRan, Skipped, Stopped};
    let asks = "node `tests` asks at its terminal";
    let refused = "node `grill` failed: session `mock` refused the request (its credential \
                   was refused): Not logged in · Please run /login";
    let quick = [Failed, NeverRan, NeverRan, Skipped, NeverRan];
    let gate = "node `approve` wants an external gate (assignee: reviewer) but no forge is \
                reachable from this machine (no credentials, or none configured): resolve it \
                here instead";
    let stopped = "interrupted by a person";
    let asking = run("Q78GRR", "standard", NeedsYou, &nodes(15, 13), asks, "0s");
    Runs {
        rows: vec![
            RunLine {
                pointed: true,
                ..asking
            },
            run("EZRPE9", "quick", NeedsYou, &quick, refused, "40d"),
            run("C0NS0E", "default", NeedsYou, &nodes(2, 1), gate, "1h"),
            run("ST0PPD", "default", Stopped, &nodes(1, 1), stopped, "1h"),
        ],
    }
}

/// The field's rows draw their target's rows from the pointer's column in the 74 cells the
/// list has at 120 columns: the mode alone in ten cells, the furrow in its column of sixteen,
/// what holds the run cut to what is left and the age at the far edge.
#[test]
fn at_120_columns_the_fields_rows_draw_their_targets_rows() {
    assert_eq!(
        drawn_at(&the_targets_runs(), 74),
        [
            "▸ ◆ Q78GRR standard  ━━━━━━━━━━━━━━┄┄ node `tests` asks at its termi…   0s",
            "  ◆ EZRPE9 quick     ━ ┄ ┄┄ ╌ ┄       node `grill` failed: session `…  40d",
            "  ◆ C0NS0E default   ━━ ┄             node `approve` wants an extern…   1h",
            "  ■ ST0PPD default   ━━               interrupted by a person           1h",
        ]
    );
}

/// The same rows in the 76 cells the list has at 80 columns: what holds each run keeps the
/// two cells more.
#[test]
fn at_80_columns_the_fields_rows_draw_their_targets_rows() {
    assert_eq!(
        drawn_at(&the_targets_runs(), 76),
        [
            "▸ ◆ Q78GRR standard  ━━━━━━━━━━━━━━┄┄ node `tests` asks at its terminal   0s",
            "  ◆ EZRPE9 quick     ━ ┄ ┄┄ ╌ ┄       node `grill` failed: session `mo…  40d",
            "  ◆ C0NS0E default   ━━ ┄             node `approve` wants an external…   1h",
            "  ■ ST0PPD default   ━━               interrupted by a person             1h",
        ]
    );
}

/// At 60 columns, in 56 cells, what holds a run would keep fewer than sixteen cells beside
/// the furrow, and the furrow gives up its column: what holds the run runs from the mode's.
#[test]
fn at_60_columns_the_furrow_gives_up_its_column() {
    assert_eq!(
        drawn_at(&the_targets_runs(), 56)[..3],
        [
            "▸ ◆ Q78GRR standard  node `tests` asks at its term…   0s",
            "  ◆ EZRPE9 quick     node `grill` failed: session…   40d",
            "  ◆ C0NS0E default   node `approve` wants an exter…   1h",
        ]
    );
}

/// A field of two groups as a page holds them: a run of `fragua` asking at its terminal, and
/// under it a run of `cancel-me` a person stopped, each list under its heading.
fn two_groups() -> (Runs, Runs) {
    use Standing::{NeedsYou, Stopped};
    let asks = "node `tests` asks at its terminal";
    let asking = Runs {
        rows: vec![run(
            "Q78GRR",
            "standard",
            NeedsYou,
            &nodes(15, 13),
            asks,
            "26m",
        )],
    };
    let stopped = Runs {
        rows: vec![RunLine {
            workflow: "cancel-me".to_string(),
            ..run(
                "ST0PPD",
                "default",
                Stopped,
                &nodes(1, 1),
                "interrupted by a person",
                "1h",
            )
        }],
    };
    (asking, stopped)
}

/// On a printed page, where no glance names the workflow, a row names it beside its mode, in a
/// column as wide as the widest name among every list of runs the page holds: the furrows of
/// two groups start at one cell.
#[test]
fn a_page_names_the_workflow_with_its_mode_lined_up_down_the_page() {
    let (asking, stopped) = two_groups();
    let page = Doc::new()
        .with(Block::Heading("asking now".to_string()))
        .with(asking)
        .with(Block::Heading("stopped".to_string()))
        .with(stopped);
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(100), None),
        motion: crate::motion::Motion::Still,
    };
    let rows: Vec<String> = Terminal::on(look)
        .lines(&page)
        .iter()
        .map(|line| line.text())
        .filter(|row| row.contains("Q78GRR") || row.contains("ST0PPD"))
        .collect();
    assert_eq!(
        rows,
        [
            "  ◆ Q78GRR fragua (standard)    ━━━━━━━━━━━━━━┄┄ node `tests` asks at its terminal               26m",
            "  ■ ST0PPD cancel-me (default)  ━━               interrupted by a person                          1h",
        ]
    );
}

/// In a screen's region the same row names the mode alone in its ten cells, as the field's
/// targets draw it: the glance beside the list names the workflow.
#[test]
fn a_region_names_the_mode_alone() {
    let (asking, _) = two_groups();
    assert_eq!(
        drawn_at(&asking, 74),
        ["  ◆ Q78GRR standard  ━━━━━━━━━━━━━━┄┄ node `tests` asks at its termi…  26m"]
    );
}

/// On a printed page the furrow's column is as wide as the widest furrow among the page's
/// lists of runs, two cells a node at most: runs of two and four nodes take eight cells, and
/// what holds each run takes the cells a column of sixteen would leave blank. A page that holds
/// a run of fifteen nodes keeps sixteen, as above.
#[test]
fn a_page_fits_the_furrow_column_to_its_widest_furrow() {
    use Standing::{NeedsYou, Stopped};
    let lint = "node `lint` failed and its 0 reroutes to `fix-lint` are exhausted, and nothing \
                runs until a person says";
    let stopped = "interrupted by a person while `review` ran its second pass over the plan";
    let asking = Runs {
        rows: vec![run(
            "Q78GRR",
            "standard",
            NeedsYou,
            &nodes(2, 1),
            lint,
            "26m",
        )],
    };
    let reviewing = Runs {
        rows: vec![RunLine {
            workflow: "cancel-me".to_string(),
            ..run("ST0PPD", "default", Stopped, &nodes(4, 1), stopped, "1h")
        }],
    };
    assert_eq!(
        page_rows(asking, reviewing),
        [
            "  ◆ Q78GRR fragua (standard)    ━━ ┄     node `lint` failed and its 0 reroutes to `fix-lint` a…  26m",
            "  ■ ST0PPD cancel-me (default)  ━ ┄┄ ┄ ┄ interrupted by a person while `review` ran its second…   1h",
        ]
    );
}

/// The rows of `first` and `second`, each list under a heading of its own, as a printed page
/// 100 cells wide lays them out.
fn page_rows(first: Runs, second: Runs) -> Vec<String> {
    let page = Doc::new()
        .with(Block::Heading("needs you, oldest first".to_string()))
        .with(first)
        .with(Block::Heading("stopped".to_string()))
        .with(second);
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(100), None),
        motion: crate::motion::Motion::Still,
    };
    Terminal::on(look)
        .lines(&page)
        .iter()
        .map(|line| line.text())
        .filter(|row| row.contains("Q78GRR") || row.contains("ST0PPD"))
        .collect()
}
