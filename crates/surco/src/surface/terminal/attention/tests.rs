use super::*;
use crate::glyphs::Glyphs;
use crate::look::Width;
use crate::paint::Paint;

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
        "◆ Q78GRR needs you: the plan waits for approva… · a go there"
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
        "◆ EKPBGE needs you: implement asks at its te… · a go there"
    );
    assert_eq!(row.fill(), Some(Fill::Tint));
}
