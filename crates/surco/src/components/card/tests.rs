//! A card is a block every surface draws: whole in a document, as Markdown and as
//! a page, in each of the three things it holds; on a terminal, in every
//! environment, as a prompt holds it with all the rows it needs; and at a pane's foot
//! on a screen, its keys in the screen's legend, in the rows each target size leaves it.

use std::path::{Path, PathBuf};

use super::{Card, CardBody, ChoiceRow, Choices, RoundRow, RoundStanding, TypedRow};
use crate::components::Legend;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/card")
}

fn option(name: &str, said: &str, cost: Option<&str>) -> ChoiceRow {
    ChoiceRow {
        name: name.to_string(),
        said: Some(said.to_string()),
        cost: cost.map(str::to_string),
        key: None,
    }
}

fn card(body: CardBody, legend: Legend) -> Card {
    Card {
        question: "plan `T001` needs a decision".to_string(),
        edge: Some("approve-plan · A32BZY".to_string()),
        about: vec!["the tasks document the planner handed over".to_string()],
        body,
        legend,
    }
}

/// Three options, the second pointed at, with where it leads.
fn choosing() -> Card {
    card(
        CardBody::Choosing {
            choices: Choices {
                rows: vec![
                    option("approve", "On to `implement`", None),
                    option(
                        "adjust",
                        "Back to `plan`: sends what you say",
                        Some("one more planning session"),
                    ),
                    option("abort", "Pause here: nothing runs", None),
                ],
                at: 1,
            },
            then: Some("plan, then this gate asks again".to_string()),
        },
        Legend::new()
            .arrows("choose")
            .key("enter", "answer")
            .key("esc", "leave it waiting"),
    )
}

/// The second option chosen, every option in sight over its words, half typed and refused
/// once.
fn saying() -> Card {
    card(
        CardBody::Saying {
            choices: Choices {
                rows: vec![
                    option("approve", "On to `implement`", None),
                    option("adjust", "Back to `plan`: sends what you say", None),
                    option("abort", "Pause here: nothing runs", None),
                ],
                at: 1,
            },
            asks: "what should the plan change".to_string(),
            typed: TypedRow {
                prompt: "> ".to_string(),
                before: "split the second task".to_string(),
                after: " in two".to_string(),
            },
            refused: Some("this option needs an answer".to_string()),
        },
        Legend::new().key("enter", "send it").key("esc", "go back"),
    )
}

/// A round of three: one answered, one typed into, one to come.
fn round() -> Card {
    card(
        CardBody::Round(vec![
            RoundRow {
                asked: "What changed?".to_string(),
                note: None,
                stands: RoundStanding::Answered("one thing".to_string()),
            },
            RoundRow {
                asked: "Any risk?".to_string(),
                note: Some("optional, assumes none".to_string()),
                stands: RoundStanding::Typing {
                    typed: TypedRow {
                        prompt: "> ".to_string(),
                        before: "the cache".to_string(),
                        after: String::new(),
                    },
                    refused: vec!["say what it risks".to_string()],
                },
            },
            RoundRow {
                asked: "How deep?".to_string(),
                note: None,
                stands: RoundStanding::Ahead,
            },
        ]),
        Legend::new()
            .key("enter", "answer")
            .key("esc", "leave it for later"),
    )
}

/// Every card, by the name its goldens carry.
fn cards() -> [(&'static str, Card); 3] {
    [
        ("choosing", choosing()),
        ("saying", saying()),
        ("round", round()),
    ]
}

#[test]
fn a_card_is_written_whole_as_markdown_and_drawn_whole_on_a_page() {
    for (name, card) in cards() {
        let doc = Doc::new().with(card);
        assert_golden(&goldens().join(format!("{name}.md")), &Markdown.draw(&doc));
        assert_golden(&goldens().join(format!("{name}.html")), &Html.body(&doc));
    }
}

/// The two bodies the terminal's own golden leaves out, in a document, in every
/// environment: a document gives a card every row it needs.
#[test]
fn a_card_in_a_document_matches_its_goldens_in_every_environment() {
    for (name, card) in cards().into_iter().filter(|(name, _)| *name != "choosing") {
        let doc = Doc::new().with(card);
        for environment in ENVIRONMENTS.iter().chain(&RICH) {
            let drawn = Terminal::on(Look::of(environment)).draw(&doc);
            assert_golden(&environment.golden(&goldens(), name), &drawn);
        }
    }
}

/// A card on a screen, its keys in the screen's legend: each option with what it does.
fn on_a_screen(question: &str, edge: &str, about: &[&str], options: &[(&str, &str)]) -> Card {
    Card {
        question: question.to_string(),
        edge: Some(edge.to_string()),
        about: about.iter().map(|about| (*about).to_string()).collect(),
        body: CardBody::Choosing {
            choices: Choices {
                rows: options
                    .iter()
                    .map(|(name, said)| option(name, said, None))
                    .collect(),
                at: 0,
            },
            then: Some("implement, then review".to_string()),
        },
        legend: Legend::new(),
    }
}

/// The scope's card: a write outside the task's scope, with why the task asks for it.
fn scope() -> Card {
    on_a_screen(
        "It asks to touch crates/cli/src/commands/pack_audit.rs",
        "implement",
        &[
            "The target-aware vendor_dir and packs_root APIs are used by project-local pack \
           audit; this path must retain its existing project behavior and compile after \
           changing the shared helpers.",
        ],
        &[
            ("grant", "the task may also touch this path, and goes on"),
            ("deny", "the task goes on within the scope it has"),
        ],
    )
}

/// `card` in a pane `cells` wide and `rows` high: each row's text.
fn in_a_pane(card: &Card, glyphs: Glyphs, cells: usize, rows: usize) -> Vec<String> {
    let look = Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: Motion::Moves,
    };
    card.drawn(&look, rows)
        .lines
        .iter()
        .map(|line| line.text())
        .collect()
}

/// The scope's card at the pane's foot of each target size, as the targets draw it: in
/// full where the rows are there, and at 60 by 14 its question cut beside its edge, no
/// blank row, and what stands under the question cut on its last row, its options whole.
#[test]
fn the_scope_card_draws_the_targets_rows_at_each_size() {
    for (size, glyphs, cells, rows) in [
        ("unicode-120", Glyphs::Unicode, 84, 7),
        ("ascii-120", Glyphs::Ascii, 84, 7),
        ("unicode-80", Glyphs::Unicode, 78, 7),
        ("unicode-60", Glyphs::Unicode, 58, 6),
    ] {
        let drawn: String = in_a_pane(&scope(), glyphs, cells, rows)
            .iter()
            .map(|row| format!("{row}\n"))
            .collect();
        assert_golden(&goldens().join(format!("scope.{size}.txt")), &drawn);
    }
}

/// The design's three questions, as `grill` asks them.
const WHERE: &str = "What does global mean in terms of where a pack is installed and which \
                     users or projects can access it?";
const HOW: &str = "How should someone request a global installation, and how does that differ \
                   from project-level installation?";
const BESIDE: &str = "How should global installations interact with project-level packs, \
                      including precedence and conflicts?";
const ASSUMED: &str =
    "optional, assumes Independent stores and locks; each operation affects the selected target";

/// `asked`, in the state `stands` says, with `note` beside it.
fn question(asked: &str, note: Option<&str>, stands: RoundStanding) -> RoundRow {
    RoundRow {
        asked: asked.to_string(),
        note: note.map(str::to_string),
        stands,
    }
}

/// `asked`, being answered from a list of `values` that say nothing more, the first
/// pointed at.
fn picked_from(asked: &str, values: &[&str]) -> RoundRow {
    let value = |name: &&str| ChoiceRow {
        name: (*name).to_string(),
        said: None,
        cost: None,
        key: None,
    };
    let choices = Choices {
        rows: values.iter().map(value).collect(),
        at: 0,
    };
    question(
        asked,
        None,
        RoundStanding::Picking {
            choices: Some(choices),
            filter: None,
            refused: Vec::new(),
        },
    )
}

/// A round on a screen, its keys in the screen's legend.
fn round_on_a_screen(asks: &str, edge: &str, rows: Vec<RoundRow>) -> Card {
    Card {
        question: asks.to_string(),
        edge: Some(edge.to_string()),
        about: Vec::new(),
        body: CardBody::Round(rows),
        legend: Legend::new(),
    }
}

/// The design's round, its first question in turn.
fn grill_asks() -> Card {
    round_on_a_screen(
        "grill asks",
        "Q78GRR",
        vec![
            picked_from(
                WHERE,
                &[
                    "The current user's platform data directory, shared by that user's projects",
                    "A fixed home-directory path such as ~/.yunta",
                    "A machine-wide directory shared across users",
                ],
            ),
            question(HOW, None, RoundStanding::Ahead),
            question(BESIDE, Some(ASSUMED), RoundStanding::Ahead),
        ],
    )
}

/// The design's round once its first question is answered.
fn grill_answered() -> Card {
    let said = "The current user's platform data directory, shared by that user's projects";
    round_on_a_screen(
        "grill asks",
        "Q78GRR",
        vec![
            question(WHERE, None, RoundStanding::Answered(said.to_string())),
            picked_from(
                HOW,
                &[
                    "--global on pack add, update, remove and list",
                    "A separate pack global command group",
                    "A global-by-default install",
                ],
            ),
            question(BESIDE, Some(ASSUMED), RoundStanding::Ahead),
        ],
    )
}

/// A round whose typed answer the engine refused, the line still in its box.
fn release_refused() -> Card {
    let typing = RoundStanding::Typing {
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: "Spring release".to_string(),
            after: String::new(),
        },
        refused: vec!["this question is `required` and nothing answers it".to_string()],
    };
    round_on_a_screen(
        "ask asks",
        "REFVSE",
        vec![
            question("What is the release called?", None, typing),
            question(
                "Which environment?",
                Some("optional, assumes staging"),
                RoundStanding::Ahead,
            ),
        ],
    )
}

/// Each round at each target size, in the cells and rows its screen gives the card: whole
/// where the rows are there, and at 60 by 14 its head given way and its questions folded.
#[test]
fn the_round_draws_the_targets_rows_at_each_size() {
    let rounds = [
        ("ask", grill_asks(), [24, 13]),
        ("ask-answered", grill_answered(), [24, 13]),
        ("ask-refused", release_refused(), [26, 14]),
    ];
    for (name, card, [wide, narrow]) in rounds {
        for (size, glyphs, cells, rows) in [
            ("unicode-120", Glyphs::Unicode, 110, wide),
            ("ascii-120", Glyphs::Ascii, 110, wide),
            ("unicode-80", Glyphs::Unicode, 78, narrow),
            ("unicode-60", Glyphs::Unicode, 58, 6),
        ] {
            let text: String = in_a_pane(&card, glyphs, cells, rows)
                .iter()
                .map(|row| format!("{row}\n"))
                .collect();
            assert_golden(&goldens().join(format!("{name}.{size}.txt")), &text);
        }
    }
}

/// A filter in force is never hidden: on the answered round at 60 by 14, whose list stands
/// without the row it is narrowed on while nothing is typed there, one letter typed brings
/// that row back with the letter on it, a choice giving way for it, and the cursor stands
/// after the letter.
#[test]
fn a_filter_typed_on_the_answered_round_stands_at_the_least_size() {
    let mut card = grill_answered();
    if let CardBody::Round(rows) = &mut card.body {
        for row in rows {
            if let RoundStanding::Picking { filter, .. } = &mut row.stands {
                *filter = Some("g".to_string());
            }
        }
    }
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(58),
        motion: Motion::Moves,
    };
    let placed = card.drawn(&look, 6);
    let rows: Vec<String> = placed.lines.iter().map(|line| line.text()).collect();
    let at = rows.iter().position(|row| row.trim_end().ends_with("▸ g"));
    let at = at.unwrap_or_else(|| panic!("no row narrowed on `g`: {rows:#?}"));
    assert_eq!(placed.cursor, Some((at, 10)), "{rows:#?}");
    assert_eq!(rows.len(), 6, "{rows:#?}");
}

/// The failure's card: two options, the first wrapping at 80 cells.
fn failure() -> Card {
    on_a_screen(
        "`tests` failed. What now?",
        "tests",
        &[],
        &[
            (
                "retry",
                "Starts a fresh attempt from the run's tree as it stands: what failed is \
                 asked again and what passed stays passed",
            ),
            (
                "abort",
                "Pause here: nothing runs; this decision stays open",
            ),
        ],
    )
}

/// The departure's card: four options, two of them wrapping at 80 cells.
fn departure() -> Card {
    on_a_screen(
        "The task departs from its plan. Accept it?",
        "implement",
        &[],
        &[
            (
                "continue-work",
                "keeps the blocked task's last changes and judges them before any session \
                 opens",
            ),
            (
                "accept",
                "the work stands as it is, and the plan records why it differs",
            ),
            ("send-back", "the session goes back to what the plan says"),
            ("abort", "nothing runs; this decision stays open"),
        ],
    )
}

/// With the rows they take wrapped, the options wrap under themselves after a blank row.
#[test]
fn with_the_rows_for_it_each_option_wraps_under_itself() {
    assert_eq!(
        in_a_pane(&failure(), Glyphs::Unicode, 78, 5),
        [
            "▎ ◆ `tests` failed. What now?                                            tests",
            "▎",
            "▎ ▸ retry  Starts a fresh attempt from the run's tree as it stands: what",
            "▎          failed is asked again and what passed stays passed",
            "▎   abort  Pause here: nothing runs; this decision stays open",
        ]
    );
    assert_eq!(
        in_a_pane(&departure(), Glyphs::Unicode, 78, 8),
        [
            "▎ ◆ The task departs from its plan. Accept it?                       implement",
            "▎",
            "▎ ▸ continue-work  keeps the blocked task's last changes and judges them",
            "▎                  before any session opens",
            "▎   accept         the work stands as it is, and the plan records why it",
            "▎                  differs",
            "▎   send-back      the session goes back to what the plan says",
            "▎   abort          nothing runs; this decision stays open",
        ]
    );
}

/// Short of the rows its options take wrapped, a card keeps every option, each on one row
/// cut at the line, and its question on one row: a card's options never give way.
#[test]
fn short_of_rows_each_option_stands_on_one_row_cut_at_the_line() {
    assert_eq!(
        in_a_pane(&failure(), Glyphs::Unicode, 58, 3),
        [
            "▎ ◆ `tests` failed. What now?                        tests",
            "▎ ▸ retry  Starts a fresh attempt from the run's tree as…",
            "▎   abort  Pause here: nothing runs; this decision stays…",
        ]
    );
    assert_eq!(
        in_a_pane(&departure(), Glyphs::Unicode, 58, 5),
        [
            "▎ ◆ The task departs from its plan. Accept it?   implement",
            "▎ ▸ continue-work  keeps the blocked task's last changes…",
            "▎   accept         the work stands as it is, and the plan…",
            "▎   send-back      the session goes back to what the plan…",
            "▎   abort          nothing runs; this decision stays open",
        ]
    );
}
