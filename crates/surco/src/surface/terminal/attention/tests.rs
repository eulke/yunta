use std::path::{Path, PathBuf};

use super::*;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS};
use crate::look::Width;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface};

/// A plain look `cells` wide, in Unicode.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(cells), None),
        motion: crate::motion::Motion::Moves,
    }
}

/// A row cut short keeps its answer: the command stands before what is asked
/// and keeps whole, and a key stands at the row's end, what is asked giving
/// way to it.
#[test]
fn an_attention_cut_short_keeps_its_answer() {
    let asks = "the plan waits for approval of every one of its seven tasks";
    let command = Attention::command(asks, "yunta resolve-gate 7E5PH4 approve");
    let key = Attention::key(asks, "a", "go there").of("Q78GRR");

    let by_command = command.lines(&look(60))[0].text();
    let by_key = key.lines(&look(60))[0].text();

    assert_eq!(
        by_command,
        "◆ needs you: yunta resolve-gate 7E5PH4 approve (the plan wa…"
    );
    assert_eq!(
        by_key,
        "◆ Q78GRR needs you: the plan waits for approv…  · a go there"
    );
}

/// What a command answers is drawn as the attached region's row is built at
/// the base, span for span: the mark and the word in their tone, the colon,
/// the command strong and what is asked muted.
#[test]
fn a_command_attention_draws_the_regions_row() {
    let attention = Attention::command("node `approve-plan`", "yunta resolve-gate 7E5PH4 approve");
    let region = Line::new()
        .push(Tone::NeedsYou, "◆ needs you")
        .plain(": ")
        .push(Tone::Strong, "yunta resolve-gate 7E5PH4 approve")
        .push(Tone::Muted, " (node `approve-plan`)");

    assert_eq!(attention.lines(&look(100)), vec![region]);
}

/// An attention that asks nothing has nothing to say, and draws nothing.
#[test]
fn an_attention_that_asks_nothing_draws_nothing() {
    assert!(Attention::command(" ", "yunta status")
        .lines(&look(80))
        .is_empty());
}

/// An attention answered at another terminal says what is asked after the
/// mark and `needs you`, plain, and a row cut short loses its end.
#[test]
fn an_attention_answered_elsewhere_says_what_is_asked() {
    let asked = Attention::elsewhere("node `implement` asks at its terminal");

    let whole = asked.lines(&look(60));
    let cut = Attention::elsewhere("node `implement` asks at its terminal")
        .of("EKPBGE")
        .lines(&Look {
            width: Width::exact(40),
            ..look(60)
        });

    assert_eq!(
        whole[0].text(),
        "◆ needs you: node `implement` asks at its terminal"
    );
    assert_eq!(
        whole[0].spans().last().map(|span| span.tone),
        Some(Tone::Plain)
    );
    assert_eq!(cut[0].text(), "◆ EKPBGE needs you: node `implement` as…");
}

/// What the design's attention row asks, which ends in a number.
const EXITS: &str = "`cargo test --workspace` passed at baseline but now exits 101";

/// A plain look exactly `cells` wide, in Unicode.
fn exactly(cells: usize) -> Look {
    Look {
        width: Width::exact(cells),
        ..look(cells)
    }
}

/// The design's attention row keeps `  · a go there` whole at its end at every width, and what
/// it asks is cut before a number it cannot keep whole, never inside it or right after it. On
/// a terminal of 80 columns, inside its margins, it is the row the targets draw.
#[test]
fn the_design_row_keeps_its_key_and_cuts_what_it_asks_before_a_number() {
    let attention = Attention::key(EXITS, "a", "go there").of("Q78GRR");
    for cells in 40..=100 {
        let row = attention.lines(&exactly(cells))[0].text();
        assert!(cell_width(&row) <= cells, "at {cells} cells: {row}");
        assert!(row.ends_with("· a go there"), "at {cells} cells: {row}");
        let cut = row.split_once('…').map(|(kept, _)| kept);
        assert!(
            !cut.is_some_and(|kept| kept.ends_with(|c: char| c.is_ascii_digit())),
            "at {cells} cells: {row}"
        );
    }
    assert_eq!(
        attention.lines(&exactly(78))[0].text(),
        "◆ Q78GRR needs you: `cargo test --workspace` passed at baseline…  · a go there"
    );
}

/// An attention said where a line is typed names no key, since `a` types there: the mark, the
/// handle and what it asks, cut at the row's end, before a number it cannot keep whole.
#[test]
fn an_unkeyed_attention_names_no_key_and_keeps_its_handle() {
    let asks = "node `grill` asks 3 questions: nothing is recorded until every one is answered";
    let row = Attention::unkeyed(asks).of("Q78GRR").lines(&exactly(58));
    assert_eq!(
        row[0].text(),
        "◆ Q78GRR needs you: node `grill` asks 3 questions: nothin…"
    );
    let narrow = Attention::unkeyed(EXITS).of("Q78GRR").lines(&exactly(80));
    assert_eq!(
        narrow[0].text(),
        "◆ Q78GRR needs you: `cargo test --workspace` passed at baseline but now exits…"
    );
}

/// On a screen, the attention is the reference's row at 58 cells, what is asked giving way
/// to the act it offers, and the row tinted across the screen.
#[test]
fn the_attention_row_keeps_its_act_and_is_tinted() {
    let attention = Attention::key("implement asks at its terminal", "a", "go there").of("EKPBGE");
    let terminal = Terminal::on(Look {
        width: Width::exact(58),
        ..look(58)
    });
    let row = terminal.attention(&attention);
    assert_eq!(
        row.text(),
        "◆ EKPBGE needs you: implement asks at its t…  · a go there"
    );
    assert_eq!(row.fill(), Some(Fill::Tint));
}

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens")
}

/// The attention row's three cases: a run's own decision, said with no subject; another
/// run's, with its handle; and one said where a line is typed, which names no key.
fn cases() -> [Attention; 3] {
    let refused = "node `grill` failed: session `mock` refused the request (its credential was \
                   refused): Not logged in · Please run /login.";
    let asked = "node `grill` asks 3 questions: nothing is recorded until every one is answered";
    [
        Attention::key(EXITS, "a", "go there"),
        Attention::key(refused, "a", "go there").of("EZRPE9"),
        Attention::unkeyed(asked).of("Q78GRR"),
    ]
}

/// The three cases on terminals of 80 and 60 columns, inside their margins, in every
/// environment, and as Markdown and as a page.
#[test]
fn the_attention_rows_match_their_goldens_on_every_surface() {
    let doc = cases().into_iter().fold(Doc::new(), Doc::with);
    assert_golden(
        &goldens().join("markdown/attention.md"),
        &Markdown.draw(&doc),
    );
    assert_golden(&goldens().join("html/attention.html"), &Html.body(&doc));
    for environment in &ENVIRONMENTS {
        let on = |cells: usize| {
            Terminal::on(Look {
                width: Width::exact(cells),
                ..Look::of(environment)
            })
        };
        let drawn: String = [78, 58]
            .into_iter()
            .flat_map(|cells| cases().map(|attention| on(cells).attention(&attention)))
            .map(|row| format!("{}\n", on(environment.width).look.paint.line(&row)))
            .collect();
        assert_golden(
            &environment.golden(&goldens().join("screen"), "attention"),
            &drawn,
        );
    }
}
