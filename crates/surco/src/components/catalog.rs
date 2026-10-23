//! Every block a document is made of, each once, drawn in every
//! environment and as Markdown: what a new surface has to draw, and what a
//! change to one block is seen against.

use std::path::{Path, PathBuf};

use super::*;
use crate::diagram::Diagram;
use crate::doc::{Block, Doc};
use crate::glyphs::{Glyphs, Mark};
use crate::golden::{assert_golden, ENVIRONMENTS};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::paint::Ink;
use crate::surface::{Markdown, Surface, Terminal};
use crate::words::Standing;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens")
}

/// What a run's decision offers, with the command that chooses each option.
fn decision() -> Decision {
    let options = vec![
        DecisionOption {
            id: "approve".to_string(),
            label: Some("builds the plan".to_string()),
            tradeoff: Some("the four tasks run in three steps".to_string()),
            asks: None,
        },
        DecisionOption {
            id: "adjust".to_string(),
            label: None,
            tradeoff: Some("the plan is written again with what you say".to_string()),
            asks: Some("what to change".to_string()),
        },
    ];
    let commands = [
        ("approve", "yunta resolve-gate 7E5PH4 approve"),
        (
            "adjust",
            "yunta resolve-gate 7E5PH4 adjust --text \"<answer>\"",
        ),
    ]
    .into_iter()
    .map(|(id, command)| (id.to_string(), command.to_string()))
    .collect();
    Decision {
        chosen: Chosen::Here { commands },
        options,
        otherwise: vec![("yunta close 7E5PH4".to_string(), "closes it for good")],
    }
}

/// The first blocks of the catalog: what a document is, what it is about,
/// and what it found.
fn opening() -> Vec<Block> {
    let mut checks = Checklist::default();
    checks.push(Found::Holds, "git", "commits as Ada <ada@example.com>");
    checks.push(Found::Caution, "forge", "github acme/web, reachable");
    checks.push(Found::NotApplicable, "review", "no fan-out review");
    vec![
        Block::Title(Line::new().push(Tone::Strong, "plan of run 7E5PH4")),
        Headline::new(Mark::NeedsYou, "fragua (standard)", "needs you")
            .fact("for 32m10s")
            .edge("7E5PH4")
            .into(),
        Furrow(vec![
            Standing::Finished,
            Standing::Finished,
            Standing::Rerouted,
            Standing::NeedsYou,
            Standing::Skipped,
            Standing::NeverRan,
        ])
        .into(),
        Gist("approve-plan asks you to approve the plan before anything is built.".to_string())
            .into(),
        Prose(
            "The run plans the change, asks before it builds, and builds what you approve."
                .to_string(),
        )
        .into(),
        Block::Heading("what it found".to_string()),
        checks.into(),
        Marked {
            mark: Mark::Caution,
            items: vec![
                "A name with a newline in it breaks the line it is printed on.".to_string(),
            ],
        }
        .into(),
        Concern {
            fact: "the budget sits under this workflow's usual spend".to_string(),
            so: "the run may stop before it finishes".to_string(),
            fix: "raise `limits.max_tokens_per_run`".to_string(),
        }
        .into(),
    ]
}

/// The blocks that quote what a run did: a section with its evidence and
/// what its failure said whole, and the paths a failure names.
fn quoting() -> Vec<Block> {
    let evidence = Evidence {
        tail: (1..=3).map(|n| format!("line {n}")).collect(),
        whole: Some(Whole::Command(
            "yunta status 7E5PH4 --node lint".to_string(),
        )),
    };
    vec![
        Section {
            mark: Some(Mark::Failed),
            title: Line::new()
                .push(Tone::Strong, "node `lint`")
                .plain(": exit 101"),
            blocks: vec![
                evidence.into(),
                Said(vec![
                    "error: unused import".to_string(),
                    "  --> src/lib.rs:3:5".to_string(),
                ])
                .into(),
            ],
        }
        .into(),
        Paths {
            heading: "outside the declared globs:".to_string(),
            paths: vec![
                PathNote {
                    path: PathBuf::from("src/main.rs"),
                    reason: None,
                },
                PathNote {
                    path: PathBuf::from("Cargo.lock"),
                    reason: Some("the lock file".to_string()),
                },
            ],
        }
        .into(),
    ]
}

/// The blocks a plan and a run's rows are drawn with, and how a document
/// closes: the decision, the facts, and what to type next.
/// Two runs of a field: one waiting on its reader, one a person stopped.
fn field() -> Runs {
    let plowed = |done: usize, now: Standing, ahead: usize| {
        let mut furrow = vec![Standing::Finished; done];
        furrow.push(now);
        furrow.extend(std::iter::repeat_n(Standing::NeverRan, ahead));
        furrow
    };
    Runs {
        rows: vec![
            RunLine {
                standing: Standing::NeedsYou,
                handle: "EKPBGE".to_string(),
                workflow: "fragua".to_string(),
                mode: "standard".to_string(),
                furrow: plowed(5, Standing::NeedsYou, 9),
                said: "implement asks at its terminal".to_string(),
                edge: Edge::Age("32m".to_string()),
            },
            RunLine {
                standing: Standing::Stopped,
                handle: "60Y934".to_string(),
                workflow: "fragua".to_string(),
                mode: "quick".to_string(),
                furrow: plowed(2, Standing::Stopped, 4),
                said: "stopped while it ran".to_string(),
                edge: Edge::Command("yunta resume 60Y934".to_string()),
            },
        ],
    }
}

fn closing() -> Vec<Block> {
    let column = |title, holds| Column { title, holds };
    let table = Table {
        columns: vec![
            column("step", Holds::Words),
            column("task", Holds::Id),
            column("", Holds::Rest),
        ],
        rows: vec![Row {
            mark: Some(Mark::Running),
            cells: vec![
                Cell::plain("1"),
                Cell::plain("cli-global"),
                Cell::plain("Global and project flags"),
            ],
        }],
    };
    let nodes = NodeTable {
        rows: vec![NodeRow {
            mark: Mark::Done,
            word: "finished",
            id: "plan".to_string(),
            note: "4 tasks".to_string(),
            took: "1m31s".to_string(),
            spent: "1.94M".to_string(),
        }],
    };
    vec![
        Block::Markdown("A change **in Markdown**, with `code`.\n\n- an item".to_string()),
        Block::Diagram(Diagram::of("graph LR\n  plan --> build")),
        Code::whole(
            "src/lib.rs",
            Some("the change".to_string()),
            "pub fn greet() {}\n",
        )
        .into(),
        table.into(),
        nodes.into(),
        field().into(),
        Block::Lines(vec![Line::new().plain("  a line a page composed itself")]),
        decision().into(),
        Note("the run holds its own state on disk.".to_string()).into(),
        Fields::new().push_if("tokens", "20.2k spent").into(),
        Next {
            steps: vec![("yunta status 7E5PH4".to_string(), "where it stands")],
        }
        .into(),
    ]
}

/// Every block the document is made of, each once.
fn catalog() -> Doc {
    opening()
        .into_iter()
        .chain(quoting())
        .chain(closing())
        .fold(Doc::new(), Doc::with)
}

#[test]
fn every_block_matches_its_goldens_in_every_environment() {
    let doc = catalog();
    for environment in &ENVIRONMENTS {
        let look = Look::of(environment);
        assert_golden(
            &environment.golden(&goldens().join("components"), "catalog"),
            &Terminal::on(look).draw(&doc),
        );
    }
    assert_golden(&goldens().join("markdown/catalog.md"), &Markdown.draw(&doc));
}

/// The words of `text` once markup, glyphs and the padding of columns
/// are gone: what carries the meaning, whichever medium drew it.
fn words(text: &str) -> std::collections::BTreeSet<String> {
    text.split_whitespace()
        .map(|token| token.trim_matches(|c| matches!(c, '*' | '_' | '`' | '|' | ':' | ',' | '.')))
        .filter(|token| token.chars().any(char::is_alphanumeric))
        .map(str::to_string)
        .collect()
}

#[test]
fn markdown_says_every_word_the_terminal_says_of_every_block() {
    let look = Look {
        glyphs: Glyphs::Unicode,
        ink: Ink::Plain,
        width: Width::of(Some(120), None),
    };
    let terminal = words(&Terminal::on(look).draw(&catalog()));
    let markdown = words(&Markdown.draw(&catalog()));
    let lost: Vec<&String> = terminal.difference(&markdown).collect();
    assert!(lost.is_empty(), "Markdown dropped {lost:?}");
}
