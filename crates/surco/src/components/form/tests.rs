//! A form is a block every surface draws: as Markdown, as a page, and on a terminal in every
//! environment. Held in a pane short of rows it gives way in its order, then keeps the row in
//! focus in a window that counts the rows it hides, and the cursor stands in the box of the
//! row in focus only where that row types. What a region beside it acts on, where that region
//! gave way, stands at its foot; what refused the act over a row stands under that row.

use std::path::{Path, PathBuf};

use super::{Form, FormFoot, FormRow, FormValue};
use crate::components::{ChoiceRow, Choices, Region, Stretch, TypedRow, WordChoice};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Paned, Surface, Terminal};
use crate::words::Standing;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/form")
}

/// The words typed before the cursor and after it, opened as an answer is.
fn typed(before: &str, after: &str) -> FormValue {
    FormValue::Typed {
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: before.to_string(),
            after: after.to_string(),
        },
        placeholder: None,
    }
}

/// A row labelled by `label` over `value`.
fn labelled(label: &str, value: FormValue) -> FormRow {
    FormRow {
        label: Some(Line::new().push(Tone::Muted, label)),
        value,
        description: None,
        refusal: None,
    }
}

/// A row of `words`, the first chosen.
fn words(words: &[&str]) -> FormValue {
    FormValue::Words(WordChoice {
        words: words.iter().map(|word| word.to_string()).collect(),
        chosen: 0,
        focused: false,
    })
}

/// A new run of a workflow with a mode, its request half typed and in focus: the request,
/// three workflows, the modes and the adapters, and what runs under them.
pub(crate) fn new_run() -> Form {
    let workflow = |name: &str, does: &str| ChoiceRow {
        name: name.to_string(),
        said: Some(does.to_string()),
        cost: None,
        key: None,
    };
    let workflows = Choices {
        rows: vec![
            workflow(
                "yunta/fragua",
                "the full cycle: questions, plan, tests, work, review, PR",
            ),
            workflow("lint-fix", "lint and format, with one fix round"),
            workflow("run-tasks", "a tasks document you wrote by hand"),
        ],
        at: 0,
    };
    let what_runs = FormFoot {
        heading: Line::new().push(Tone::Muted, "what runs"),
        strokes: vec![
            Stretch::new("grill", Standing::NeverRan),
            Stretch::new("plan", Standing::NeverRan),
            Stretch::new("review", Standing::Skipped),
            Stretch::new("ship", Standing::NeverRan),
        ],
        said: vec![
            Line::new().plain("grill plan ship"),
            Line::new().plain("it stops for you at ship"),
        ],
    };
    Form {
        asked: Some(Line::new().push(Tone::Strong, "What do you want to build?")),
        hint: Some("this becomes the brief; fragua asks only what it cannot decide for you".into()),
        rows: vec![
            FormRow {
                label: None,
                value: typed("add support", ""),
                description: None,
                refusal: None,
            },
            labelled("workflow", FormValue::Listed(workflows)),
            labelled("mode", words(&["quick", "standard", "full"])),
            labelled(
                "adapter",
                words(&["as the runners in config.yaml say", "claude-code", "codex"]),
            ),
        ],
        focus: Some(0),
        foot: Some(what_runs),
    }
}

/// The same new run of a workflow with no mode and an input beside its request, the input in
/// focus and nothing typed in it.
fn new_run_with_an_input() -> Form {
    let mut form = new_run();
    form.rows[2] = FormRow {
        description: Some("Tasks document YAML, relative to the repository root.".to_string()),
        ..labelled("tasks (document, required)", typed("", ""))
    };
    form.focus = Some(2);
    form
}

/// A plain Unicode look exactly `cells` wide.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: crate::motion::Motion::Still,
    }
}

/// `form` held in a pane `rows` high and `cells` wide.
fn held(form: Form, cells: usize, rows: usize) -> Paned {
    Terminal::on(look(cells)).pane(&Region::new(Doc::new().with(form)), rows)
}

fn texts(paned: &Paned) -> Vec<String> {
    paned.lines.iter().map(Line::text).collect()
}

#[test]
fn a_form_matches_its_goldens_on_every_surface() {
    for (case, form) in [
        ("new-run", new_run()),
        ("an-input", new_run_with_an_input()),
    ] {
        let doc = Doc::new().with(form);
        assert_golden(&goldens().join(format!("{case}.md")), &Markdown.draw(&doc));
        assert_golden(&goldens().join(format!("{case}.html")), &Html.body(&doc));
        for environment in ENVIRONMENTS.iter().chain(&RICH) {
            let drawn = Terminal::on(Look::of(environment)).draw(&doc);
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}

/// With the rows for all of it, a form opens on a blank row, a blank row parting each two of
/// its rows and what they come to, and the cursor stands after the words typed in the box of
/// the row in focus.
#[test]
fn a_form_with_the_rows_for_it_stands_whole_with_the_cursor_where_it_types() {
    let paned = held(new_run(), 40, 30);
    let drawn = texts(&paned);
    assert_eq!(drawn[0], "");
    assert_eq!(drawn[1], "What do you want to build?");
    assert_eq!(drawn[3], format!("│ add support{} │", " ".repeat(25)));
    assert_eq!(paned.cursor, Some((3, "│ add support".chars().count())));
    let headings: Vec<usize> = ["workflow", "mode", "adapter", "what runs"]
        .iter()
        .map(|heading| drawn.iter().position(|row| row == heading).expect(heading))
        .collect();
    for at in headings {
        assert_eq!(drawn[at - 1], "", "a blank row over {}", drawn[at]);
    }
}

/// A form held short of rows gives way step by step, each only where the one before does not
/// fit: in 20 rows, 22 of all of it, the blank row it opens on and the blank row between the
/// mode and the adapter give way, and the blank rows around the list and over what the rows
/// come to stand; in 16, what the rows come to gives way whole.
#[test]
fn a_form_short_of_rows_gives_way_in_its_order() {
    let drawn = texts(&held(new_run(), 78, 20));
    assert_eq!(drawn.len(), 20);
    assert_eq!(drawn[0], "What do you want to build?");
    let at = |heading: &str| drawn.iter().position(|row| row == heading).expect(heading);
    assert_eq!(drawn[at("workflow") - 1], "");
    assert_eq!(
        drawn[at("mode") - 1],
        "",
        "a list keeps the blank row under it"
    );
    assert_eq!(
        at("adapter"),
        at("mode") + 2,
        "two rows of one line stand close"
    );
    assert_eq!(drawn[at("what runs") - 1], "");

    let shorter = texts(&held(new_run(), 78, 16));
    assert_eq!(shorter.len(), 15);
    assert!(
        !shorter.iter().any(|row| row == "what runs"),
        "{shorter:#?}"
    );
    assert!(shorter.iter().any(|row| row.starts_with("this becomes")));
}

/// Where the rows do not fit even close, a window keeps the row in focus in sight and counts
/// the rows it hides under it; the form's question gives way where a row needs its row, and
/// the hint and what the rows come to stand nowhere.
#[test]
fn a_form_too_tall_for_its_pane_keeps_the_row_in_focus_in_a_window() {
    let paned = held(new_run(), 58, 10);
    assert_eq!(
        texts(&paned),
        [
            format!("╭{}╮", "─".repeat(56)),
            format!("│ add support{} │", " ".repeat(43)),
            format!("╰{}╯", "─".repeat(56)),
            "workflow".to_string(),
            "▸ yunta/fragua   the full cycle: questions, plan, tests,…".to_string(),
            "  lint-fix       lint and format, with one fix round".to_string(),
            "  run-tasks      a tasks document you wrote by hand".to_string(),
            "mode".to_string(),
            " ▸ quick   standard   full".to_string(),
            "… 1 under".to_string(),
        ]
    );
    assert_eq!(paned.cursor, Some((1, "│ add support".chars().count())));
}

/// A window that keeps every row it holds beside the form's question keeps it over the first
/// row; one that opens past the first row counts the rows above it; and the cursor stands in
/// the box of an input in focus, never in the request's.
#[test]
fn a_window_keeps_the_question_where_no_row_needs_its_row() {
    let mut on_the_workflows = new_run_with_an_input();
    on_the_workflows.focus = Some(1);
    let drawn = texts(&held(on_the_workflows, 58, 10));
    assert_eq!(drawn[0], "What do you want to build?");
    assert_eq!(drawn.last().map(String::as_str), Some("… 2 under"));
    assert_eq!(drawn.len(), 9);

    let paned = held(new_run_with_an_input(), 58, 10);
    let drawn = texts(&paned);
    assert_eq!(drawn[0], "… 2 above");
    assert_eq!(drawn[1], "tasks (document, required)");
    assert_eq!(paned.cursor, Some((3, "│ ".chars().count())));
}

/// The focus on a row that types nothing leaves no cursor anywhere, the request's box among
/// them.
#[test]
fn a_row_that_types_nothing_in_focus_leaves_no_cursor() {
    let mut on_the_mode = new_run();
    on_the_mode.focus = Some(2);
    assert_eq!(held(on_the_mode, 78, 30).cursor, None);
}

/// A form's foot stands under its heading a step in: its strokes two cells a step at most,
/// however wide the row, as a run's row in a list draws its furrow, then what it says. Fifteen
/// steps, the fourth, the fifth, the seventh, the eighth and the tenth left out, take thirty
/// cells in two rows' widths alike, the eighth stroke two cells wide.
#[test]
fn a_forms_foot_stands_a_step_in_its_strokes_two_cells_a_step() {
    let left_out = [3, 4, 6, 7, 9];
    let fifteen = || Form {
        foot: Some(FormFoot {
            strokes: (0..15)
                .map(|at| match left_out.contains(&at) {
                    true => Stretch::new(format!("node-{at}"), Standing::Skipped),
                    false => Stretch::new(format!("node-{at}"), Standing::NeverRan),
                })
                .collect(),
            ..new_run().foot.expect("a foot")
        }),
        ..new_run()
    };
    for cells in [71, 58] {
        let drawn = texts(&held(fifteen(), cells, 30));
        let at = drawn
            .iter()
            .position(|row| row == "what runs")
            .expect("its heading");
        assert_eq!(
            drawn[at + 1],
            "  ┄ ┄ ┄ ╌ ╌ ┄ ╌ ╌╌ ┄ ╌ ┄ ┄ ┄ ┄ ┄",
            "{cells} cells"
        );
        assert_eq!(drawn[at + 2], "  grill plan ship");
        assert_eq!(drawn[at + 3], "  it stops for you at ship");
    }
}

/// What a region beside a form acts on, held in the form's pane where that region gave way:
/// four rows of it.
fn stead() -> Vec<Block> {
    vec![Block::Lines(vec![Line::new().plain("in the way"); 4])]
}

/// A form whose pane holds what a region beside it acts on holds it at its foot: in 20 rows of
/// 78 cells, apart from the rows by a blank row, the form giving way first as far as its rows
/// stand close around its list and what they come to is gone; in 10 rows of 58 cells, standing
/// close, a row of the form's window, counted with the rows the window hides.
#[test]
fn a_form_holds_what_a_region_beside_it_acts_on_at_its_foot() {
    let region = Region::new(Doc::new().with(new_run()));
    let aired = Terminal::on(look(78)).pane_holding(&region, &stead(), 20, false);
    let drawn = texts(&aired);
    assert_eq!(drawn.len(), 20);
    assert_eq!(drawn[0], "What do you want to build?");
    assert!(!drawn.iter().any(|row| row == "what runs"), "{drawn:#?}");
    assert_eq!(drawn[15], "");
    assert_eq!(drawn[16..], ["in the way"; 4]);

    let close = Terminal::on(look(58)).pane_holding(&region, &stead(), 10, true);
    let drawn = texts(&close);
    assert_eq!(drawn.len(), 10);
    assert_eq!(drawn[3], "workflow");
    assert_eq!(drawn[7], "mode");
    assert_eq!(
        drawn[9], "… 2 under",
        "the adapter and what stands in the way"
    );
}

/// What refused the act over a row stands under the row's description, opened by the mark of a
/// failure, its words wrapped under the mark; Markdown says it under the same mark.
#[test]
fn a_refusal_stands_under_the_row_it_refused() {
    let refusal = "input `tasks` is required and has no default; pass `--input tasks=...`";
    let mut refused = new_run_with_an_input();
    refused.rows[2].refusal = Some(refusal.to_string());
    let drawn = texts(&held(refused, 71, 40));
    let described = "Tasks document YAML, relative to the repository root.";
    let at = drawn
        .iter()
        .position(|row| row == described)
        .expect("its description");
    assert_eq!(
        drawn[at + 1..at + 3],
        [
            "✗ input `tasks` is required and has no default; pass `--input",
            "  tasks=...`"
        ]
    );

    let mut refused = new_run_with_an_input();
    refused.rows[2].refusal = Some(refusal.to_string());
    let markdown = Markdown.draw(&Doc::new().with(refused));
    assert!(
        markdown.contains("✗ input `tasks` is required"),
        "{markdown}"
    );
}

/// Markdown says in words where each step of what runs stands, as it says a ribbon: the
/// steps the mode leaves out by name, which the strokes alone show at a terminal and the
/// names of what runs leave unsaid.
#[test]
fn markdown_names_the_steps_the_mode_leaves_out() {
    let written = Markdown.draw(&Doc::new().with(Block::Form(new_run())));
    let skipped = Standing::Skipped.word();
    assert!(written.contains(&format!("review {skipped}")), "{written}");
}
