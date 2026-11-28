use crate::components::{
    AtAGlance, CardBody, Choices, Chosen, Decision, DecisionOption, Feed, FeedRow, Fields, Fold,
    Next, Note, Prose, Region, Section, Sight, Title, TypedRow,
};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::terminal::card::tests::{card, eight_rows};
use crate::surface::Terminal;

/// What `doc` draws plain at `cells`, line by line.
fn drawn(doc: &Doc, cells: usize) -> Vec<String> {
    let look = Look {
        width: Look::plain().width.within(cells),
        ..Look::plain()
    };
    Terminal::on(look)
        .lines(doc)
        .iter()
        .map(|line| line.text().trim_end().to_string())
        .collect()
}

/// A title too long for its line breaks between two facts, never inside
/// one, and goes on under the first fact.
#[test]
fn a_title_breaks_between_its_facts_and_goes_on_under_the_first() {
    let title = Title {
        name: Line::new().push(Tone::Strong, "plan"),
        facts: vec![
            "4 tasks in 3 steps".to_string(),
            "2 risks".to_string(),
            "2 left out".to_string(),
        ],
    };
    assert_eq!(
        drawn(&Doc::new().with(title), 30),
        ["plan: 4 tasks in 3 steps,", "      2 risks, 2 left out"]
    );
}

/// A terminal opens every fold: it draws one as the section it holds.
#[test]
fn a_fold_is_drawn_as_its_section() {
    let section = || Section {
        lead: None,
        title: Line::new().plain("card"),
        blocks: vec![Block::from(Prose("inside".to_string()))],
    };
    assert_eq!(
        drawn(&Doc::new().with(Fold(section())), 60),
        drawn(&Doc::new().with(section()), 60)
    );
}

/// A decision answered on a forge ends with what is typed elsewhere: its
/// title, then each step under it.
#[test]
fn a_decision_on_a_forge_ends_with_what_is_typed_elsewhere() {
    let decision = Decision {
        chosen: Chosen::OnTheForge {
            forge: "GitHub".to_string(),
            elsewhere: "on the machine".to_string(),
        },
        options: vec![DecisionOption {
            id: "approve".to_string(),
            label: Some("approve it".to_string()),
            tradeoff: Some("it passes".to_string()),
            asks: None,
        }],
        otherwise: vec![("yunta resume x".to_string(), "reads it")],
    };
    let lines: Vec<String> = drawn(&Doc::new().with(decision), 60)
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let [.., elsewhere, step] = lines.as_slice() else {
        panic!("a forge's decision draws its options and what follows: {lines:?}");
    };
    // Every line of a decision opens with its edge.
    assert_eq!(elsewhere.trim(), "| on the machine", "{lines:?}");
    assert!(
        step.trim().starts_with("|   yunta resume x") && step.trim().ends_with("reads it"),
        "{lines:?}"
    );
}

/// Inside a section, a fold leaves the blank line a section leaves before the
/// block that follows it.
#[test]
fn a_fold_in_a_section_is_spaced_from_what_follows_it_as_a_section_is() {
    let outer = Section {
        lead: None,
        title: Line::new().plain("outer"),
        blocks: vec![
            Fold(Section {
                lead: None,
                title: Line::new().plain("card"),
                blocks: vec![Block::from(Prose("inside".to_string()))],
            })
            .into(),
            Prose("after".to_string()).into(),
        ],
    };
    assert_eq!(
        drawn(&Doc::new().with(outer), 60),
        ["  outer", "", "    card", "      inside", "", "    after"]
    );
}

/// A gloss too long to stand beside its command at the width goes on the line
/// under it, one step in, so no line passes the width; where it fits, it stands
/// beside the command.
#[test]
fn a_gloss_too_long_for_the_line_goes_under_its_command() {
    let next = || Next {
        steps: vec![(
            "yunta resume 7E5PH4".to_string(),
            "offers a retry once the run's tree changed",
        )],
    };
    assert_eq!(
        drawn(&Doc::new().with(next()), 60),
        [
            "next",
            "  yunta resume 7E5PH4",
            "    offers a retry once the run's tree changed",
        ]
    );
    assert_eq!(
        drawn(&Doc::new().with(next()), 80),
        [
            "next",
            "  yunta resume 7E5PH4   offers a retry once the run's tree changed",
        ]
    );
}

/// A gloss stands at the column its widest command sets, counted in cells: a
/// command of wide characters pads to that column by cells too.
#[test]
fn a_gloss_stands_at_its_column_counted_in_cells() {
    let next = Next {
        steps: vec![
            ("cat 数据".to_string(), "shows it"),
            ("ls".to_string(), "lists it"),
        ],
    };
    assert_eq!(
        drawn(&Doc::new().with(next), 80),
        ["next", "  cat 数据   shows it", "  ls         lists it"]
    );
}

/// A page's facts and a run's at a glance stand in one label column: a label wider than the
/// column widens it for every row, so every value stands in one column, and a value's later
/// rows stand under its first cell.
#[test]
fn a_pages_facts_and_a_glance_stand_in_one_label_column() {
    let value = "a value long enough to go on under its first cell";
    let facts = Fields::new()
        .push_if("tokens", "20.2k")
        .push_if("what it hands over", value);
    let glance = AtAGlance {
        facts: vec![
            ("tokens", "20.2k".to_string()),
            ("what it hands over", value.to_string()),
        ],
        acts: None,
    };
    let on_the_page = drawn(&Doc::new().with(facts), 60);
    let at_a_glance = drawn(&Doc::new().with(glance), 60);
    assert_eq!(on_the_page, at_a_glance);
    let at = "  what it hands over ".len();
    assert_eq!(on_the_page[0].find("20.2k"), Some(at), "{on_the_page:#?}");
    assert_eq!(on_the_page[1].find("a value"), Some(at), "{on_the_page:#?}");
    assert!(
        on_the_page[2].starts_with(&" ".repeat(at)),
        "{on_the_page:#?}"
    );
}

/// The ways a block stands in a region: alone, under a sentence and over a note, under the
/// name in block letters as a setup's rows do, over the field in perspective, and over a feed.
const WAYS: [&str; 5] = ["alone", "between", "named", "over the field", "fed"];

/// A feed of sixteen calls at its end, the newest strong.
fn sixteen_calls() -> Feed {
    let rows = (1..=16).map(|at| Line::new().plain(format!("read    crates/file-{at}.rs")));
    Feed {
        rows: rows.map(FeedRow::of).collect(),
        before: 0,
        after: 0,
    }
}

/// `block` standing in a region as `way` says.
fn standing(block: Block, way: &str) -> Doc {
    match way {
        "between" => Doc::new()
            .with(Prose("What the block is about.".to_string()))
            .with(block)
            .with(Note("what stands under it".to_string())),
        "named" => Doc::new().with(crate::components::Wordmark).with(block),
        "over the field" => Doc::new()
            .with(block)
            .with(crate::components::Horizon { tone: Tone::Done }),
        "fed" => Doc::new()
            .with(block)
            .with(Block::Heading("what it is doing".to_string()))
            .with(sixteen_calls()),
        _ => Doc::new().with(block),
    }
}

/// The variants of a region a screen draws: from its top, along its pointer, at its end,
/// titled from its top and at its end, what it holds said again in fewer rows, and its words
/// set to a measure narrower than the pane.
const VARIANTS: [&str; 7] = [
    "from its top",
    "along its pointer",
    "at its end",
    "titled",
    "titled, at its end",
    "abridged",
    "measured",
];

/// `region` as `variant` says.
fn varied(region: Region, variant: &str) -> Region {
    let titled = |region: Region| region.titled(["build", "bash"]).edge("claude-code");
    let shorter = vec![
        vec![
            Block::Rows(vec![Line::new().plain("said in two rows")]),
            Note("and a note".to_string()).into(),
        ],
        vec![Block::Rows(vec![Line::new().plain("said in one")])],
    ];
    match variant {
        "along its pointer" => region.in_sight(Sight::following_pointer()),
        "at its end" => region.in_sight(Sight::at_the_end()),
        "titled" => titled(region),
        "titled, at its end" => titled(region).in_sight(Sight::at_the_end()),
        "abridged" => region.abridged(shorter).in_sight(Sight::at_the_end()),
        "measured" => region.measured(crate::components::Measure::of(40)),
        _ => region,
    }
}

/// Prompts typed into, the cursor standing in each: a line typed in place on a setup's rows,
/// and words said for a card's option pointed at, the words refused once.
fn typed_into(glyphs: Glyphs) -> [Block; 2] {
    let catalog = crate::components::catalog::catalog().into_blocks();
    let survey = catalog.into_iter().find_map(|block| match block {
        Block::Survey(survey) => Some(survey),
        _ => None,
    });
    let mut survey = survey.expect("the catalog's survey");
    for row in survey.lists.iter_mut().flat_map(|list| &mut list.rows) {
        if let Some(typed) = row.typed.as_mut() {
            typed.focused = true;
        }
    }
    let saying = CardBody::Saying {
        choices: Choices {
            rows: eight_rows(),
            at: 6,
        },
        asks: "what should change?".to_string(),
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: "keep the old flag".to_string(),
            after: String::new(),
        },
        refused: Some("say what should change".to_string()),
    };
    [survey.into(), card(saying, glyphs).into()]
}

/// Every region a pane is held to its rows with, named: each block of the catalog and each
/// prompt typed into, in each way it stands, each in every variant.
fn every_region(glyphs: Glyphs) -> Vec<(String, Region)> {
    let mut regions: Vec<(String, Region)> = Vec::new();
    for variant in VARIANTS {
        for way in WAYS {
            let blocks = crate::components::catalog::catalog().into_blocks();
            let blocks = blocks.into_iter().chain(typed_into(glyphs));
            for (at, block) in blocks.enumerate() {
                let region = Region::new(standing(block, way));
                regions.push((
                    format!("block {at} {way}, {variant}"),
                    varied(region, variant),
                ));
            }
        }
    }
    regions
}

/// What a person acts on from a region beside the main one that gave way, held in its stead.
fn instead() -> Vec<Block> {
    vec![
        Block::Heading("what stands in its way".to_string()),
        Block::Rows(vec![
            Line::new().plain("the suite is not set"),
            Line::new().plain("enter sets it"),
        ]),
    ]
}

/// Each region drawn `cells` wide in `glyphs` that does not keep to its rows, in any way a
/// screen draws one, at any count of rows from none up to more than it takes: more lines than
/// the pane has, or the cursor on none of them.
fn past_their_rows(glyphs: Glyphs, cells: usize) -> Vec<String> {
    let terminal = Terminal::on(Look {
        glyphs,
        width: Width::exact(cells),
        ..Look::plain()
    });
    // Past the rows a region takes whole at the narrowest measure it is set to, with what it
    // holds in another's stead and a title's two, nothing it draws changes.
    let narrowest = Terminal::on(Look {
        width: Width::exact(cells.min(40)),
        ..terminal.look
    });
    let instead = instead();
    let mut broken: Vec<String> = Vec::new();
    for (name, region) in every_region(glyphs) {
        let whole = narrowest.lines(&region.doc).len() + instead.len() + 4;
        for rows in 0..=whole {
            let drawn = [
                ("pane", terminal.pane(&region, rows)),
                ("short of rows", terminal.pane_short_of_rows(&region, rows)),
                (
                    "holding",
                    terminal.pane_holding(&region, &instead, rows, false),
                ),
                (
                    "holding, short of rows",
                    terminal.pane_holding(&region, &instead, rows, true),
                ),
                (
                    "in place of its title",
                    terminal.pane_in_place_of_its_title(&region, rows, true),
                ),
            ];
            for (way, paned) in drawn {
                let over = paned.lines.len() > rows;
                let astray = paned
                    .cursor
                    .is_some_and(|(row, _)| row >= paned.lines.len());
                if over || astray {
                    broken.push(format!(
                        "{name}, {way}, {rows} rows: {} lines, the cursor at {:?}",
                        paned.lines.len(),
                        paned.cursor
                    ));
                }
            }
        }
    }
    broken
}

/// Every region keeps to its rows at the least terminal's width: never more lines than the
/// pane has, and the cursor, where one stands, on one of them.
#[test]
fn every_region_keeps_to_its_rows_at_the_least_width() {
    let broken = past_their_rows(Glyphs::Unicode, 60);
    assert!(
        broken.is_empty(),
        "{}:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

/// Every region keeps to its rows in the ASCII glyphs, as wide as a terminal of 80 columns.
#[test]
fn every_region_keeps_to_its_rows_in_ascii() {
    let broken = past_their_rows(Glyphs::Ascii, 80);
    assert!(
        broken.is_empty(),
        "{}:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

/// Every region keeps to its rows as wide as the widest terminal the design draws.
#[test]
fn every_region_keeps_to_its_rows_at_the_widest() {
    let broken = past_their_rows(Glyphs::Unicode, 120);
    assert!(
        broken.is_empty(),
        "{}:\n{}",
        broken.len(),
        broken.join("\n")
    );
}
