//! A survey is a block every surface draws: as Markdown, as a page, and on a terminal. Held in
//! a pane short of rows under the name in block letters, it gives way in its order (the blank
//! row under its sentence, then what stands above it, then a window), its act's row never
//! leaving sight; its window keeps the row pointed at in sight and counts the rows it hides,
//! never the headings nor the sentence. A line typed in place stands in a box named in its top
//! edge where the rows' values stand, the cursor in it only where the keys type there; a row of
//! words with one chosen sets its chosen word there too. Set to a measure narrower than its
//! pane, its sentence stands whole across the column, centred over it.

use super::{Survey, SurveyList, SurveyRow, SurveyValue, TypedInPlace};
use crate::components::{Measure, Region, TypedRow, WordChoice, Wordmark};
use crate::doc::{Block, Doc};
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::{Markdown, Surface, Terminal};

/// A row found, holding.
fn found(label: &str, said: &str) -> SurveyRow {
    SurveyRow {
        mark: Mark::Done,
        label: label.to_string(),
        value: SurveyValue::Found(said.to_string()),
        pointed: false,
        typed: None,
    }
}

/// A role on `candidates`, composed as its caller says them.
fn role(label: &str, candidates: &str) -> SurveyRow {
    SurveyRow {
        value: SurveyValue::Composed(Line::new().plain(candidates)),
        ..found(label, "")
    }
}

/// A survey of a repository: its sentence, three facts, three roles, the second changed in
/// place, its adapter being chosen over its model's box, and the act's row.
pub(crate) fn a_role_changed() -> Survey {
    let changed = SurveyRow {
        value: SurveyValue::Chosen(WordChoice {
            words: vec!["claude-code".to_string(), "codex".to_string()],
            chosen: 0,
            focused: true,
        }),
        pointed: true,
        typed: Some(TypedInPlace {
            label: "model".to_string(),
            typed: TypedRow {
                prompt: "> ".to_string(),
                before: String::new(),
                after: String::new(),
            },
            cells: 20,
            hint: Some("enter with none leaves it to the adapter".to_string()),
            focused: false,
        }),
        ..found("executor", "")
    };
    Survey {
        said: Some(Line::new().push(
            Tone::Muted,
            "This repository has never used yunta. Nothing is written until you say so.",
        )),
        lists: vec![
            SurveyList {
                heading: "what this repository is".to_string(),
                rows: vec![
                    found("name", "yunta"),
                    found("language", "rust"),
                    found("suite", "cargo test"),
                ],
            },
            SurveyList {
                heading: "who does the work".to_string(),
                rows: vec![
                    role("planner", "claude-code"),
                    changed,
                    role("mechanical", "claude-code"),
                ],
            },
        ],
        foot: Line::new().plain(" write .yunta/config.yaml"),
    }
}

/// A terminal `cells` wide in `glyphs`, unpainted.
fn terminal(cells: usize, glyphs: Glyphs) -> Terminal {
    Terminal::on(Look {
        glyphs,
        width: Width::exact(cells),
        ..Look::plain()
    })
}

/// `lines`' texts, each without the blank cells it ends with.
fn texts(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.text().trim_end().to_string())
        .collect()
}

/// The survey under the name in block letters, as a region a screen opens on.
fn under_the_name(survey: Survey) -> Region {
    Region::new(Doc::new().with(Wordmark).with(Block::Survey(survey)))
}

#[test]
fn a_survey_says_its_sentence_its_lists_and_its_act_with_a_row_changed_in_place() {
    let drawn = texts(
        &terminal(60, Glyphs::Unicode).lines(&Doc::new().with(Block::Survey(a_role_changed()))),
    );
    let expected = [
        "This repository has never used yunta. Nothing is written",
        "until you say so.",
        "",
        "what this repository is",
        "  ✓ name        yunta",
        "  ✓ language    rust",
        "  ✓ suite       cargo test",
        "",
        "who does the work",
        "  ✓ planner     claude-code",
        "▸ ✓ executor  ▸ claude-code   codex",
        "                ╭ model ───────────╮",
        "                │                  │",
        "                ╰──────────────────╯",
        "                enter with none leaves it to the adapter",
        "  ✓ mechanical  claude-code",
        "",
        " write .yunta/config.yaml",
    ];
    assert_eq!(drawn, expected, "{drawn:#?}");
    let written = Markdown.draw(&Doc::new().with(Block::Survey(a_role_changed())));
    assert!(
        written.contains("▸ ✓ **executor** ▸ claude-code  codex"),
        "{written}"
    );
    assert!(written.contains("✓ **name** yunta"), "{written}");
}

/// Short of rows, the blank row under the sentence gives way first, then the name in block
/// letters, then a window opens that keeps the act's row and the row pointed at and counts
/// the rows it hides above, never on a blank row.
#[test]
fn short_of_rows_a_survey_gives_way_in_its_order_and_counts_what_its_window_hides() {
    let terminal = terminal(60, Glyphs::Unicode);
    let region = under_the_name(a_role_changed());
    let whole = texts(&terminal.pane(&region, 21).lines);
    assert_eq!(
        whole[3],
        "This repository has never used yunta. Nothing is written"
    );
    assert_eq!(whole[5], "", "the blank row under the sentence stands");
    let close = texts(&terminal.pane(&region, 20).lines);
    assert_eq!(close[5], "what this repository is", "{close:#?}");
    let alone = texts(&terminal.pane(&region, 17).lines);
    assert_eq!(
        alone[0],
        "This repository has never used yunta. Nothing is written"
    );
    let windowed = texts(&terminal.pane(&region, 10).lines);
    let expected = [
        "… 3 above",
        "  ✓ planner     claude-code",
        "▸ ✓ executor  ▸ claude-code   codex",
        "                ╭ model ───────────╮",
        "                │                  │",
        "                ╰──────────────────╯",
        "                enter with none leaves it to the adapter",
        "  ✓ mechanical  claude-code",
        "",
        " write .yunta/config.yaml",
    ];
    assert_eq!(windowed, expected, "{windowed:#?}");
    let tighter = texts(&terminal.pane(&region, 8).lines);
    assert_eq!(
        tighter[0], "… 4 above",
        "a heading is never counted: {tighter:#?}"
    );
    assert_eq!(
        tighter.last().map(String::as_str),
        Some(" write .yunta/config.yaml")
    );
}

/// A row pointed at taller than what the window leaves beside its counts gives way from its
/// end, its hint first and then its box's rows, so the survey keeps to its rows and its act's
/// row stays the last of them; the cursor stands only on a row it keeps.
#[test]
fn a_row_pointed_at_taller_than_its_window_gives_way_from_its_end_before_the_act() {
    let terminal = terminal(60, Glyphs::Unicode);
    let region = under_the_name(a_role_changed());
    let drawn = texts(&terminal.pane(&region, 7).lines);
    assert_eq!(
        drawn,
        [
            "… 4 above",
            "▸ ✓ executor  ▸ claude-code   codex",
            "                ╭ model ───────────╮",
            "                │                  │",
            "                ╰──────────────────╯",
            "… 1 under",
            " write .yunta/config.yaml",
        ],
        "{drawn:#?}"
    );
    let typing = |rows: usize| {
        let mut survey = a_role_changed();
        if let Some(typed) = survey.lists[1].rows[1].typed.as_mut() {
            typed.focused = true;
        }
        terminal.pane(&under_the_name(survey), rows)
    };
    let kept = typing(6);
    assert_eq!(
        texts(&kept.lines)[1..=3],
        [
            "▸ ✓ executor  ▸ claude-code   codex",
            "                ╭ model ───────────╮",
            "                │                  │",
        ]
    );
    assert_eq!(kept.cursor.map(|(row, _)| row), Some(3));
    let lost = typing(5);
    assert_eq!(lost.lines.len(), 5, "{:#?}", texts(&lost.lines));
    assert_eq!(lost.cursor, None, "{:#?}", texts(&lost.lines));
}

/// A row pointed at above what a window reaching down to the act's row holds: the window
/// opens on it and counts what it hides under it too, the act's row still last.
#[test]
fn a_window_opens_on_a_row_pointed_at_high_and_counts_what_it_hides_under() {
    let mut survey = a_role_changed();
    survey.lists[1].rows[1].pointed = false;
    survey.lists[0].rows[0].pointed = true;
    let drawn = texts(
        &terminal(60, Glyphs::Unicode)
            .pane(&under_the_name(survey), 5)
            .lines,
    );
    assert_eq!(
        drawn,
        [
            "▸ ✓ name        yunta",
            "  ✓ language    rust",
            "  ✓ suite       cargo test",
            "… 3 under",
            " write .yunta/config.yaml",
        ],
        "{drawn:#?}"
    );
}

/// The cursor stands in a line typed in place only where the keys type there: after the
/// words before it, in the box's middle row.
#[test]
fn the_cursor_stands_in_a_line_typed_in_place_only_where_the_keys_type_there() {
    let region = |focused: bool| {
        let mut survey = a_role_changed();
        if let Some(typed) = survey.lists[1].rows[1].typed.as_mut() {
            typed.focused = focused;
            typed.typed.before = "opus".to_string();
        }
        survey.lists[1].rows[1].value = SurveyValue::Found("claude-code".to_string());
        Region::new(Doc::new().with(Block::Survey(survey)))
    };
    let terminal = terminal(60, Glyphs::Ascii);
    assert_eq!(terminal.pane(&region(false), 30).cursor, None);
    let held = terminal.pane(&region(true), 30);
    let (row, cell) = held.cursor.expect("the cursor in the box");
    let drawn = texts(&held.lines);
    assert_eq!(drawn[row - 1], "                . model -----------.");
    assert_eq!(drawn[row], "                | opus             |");
    assert_eq!(cell, 16 + 2 + 4);
}

/// Set to a measure narrower than its pane, a survey stands in the measure's column, centred,
/// and its sentence, wider than the column, stands whole over it, centred too, the odd cell at
/// the left; its words stand where they fall, never lowered.
#[test]
fn set_to_a_measure_its_sentence_stands_whole_across_the_column() {
    let mut survey = a_role_changed();
    survey.said = Some(Line::new().plain("x".repeat(45)));
    let region = Region::new(Doc::new().with(Block::Survey(survey))).measured(Measure::of(40));
    let drawn = texts(&terminal(60, Glyphs::Unicode).pane(&region, 40).lines);
    assert_eq!(
        drawn[0],
        format!("{}{}", " ".repeat(7), "x".repeat(45)),
        "{drawn:#?}"
    );
    assert_eq!(
        drawn[2],
        format!("{}what this repository is", " ".repeat(10))
    );
}
