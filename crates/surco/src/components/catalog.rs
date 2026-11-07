//! Every block a document is made of, each once, drawn in every
//! environment, the 24-bit ones among them, and as Markdown: what a new
//! surface has to draw, and what a change to one block is seen against.

mod closing;
mod exact;
mod kinds;
mod on_screen;

use std::path::{Path, PathBuf};

use super::tests::words;
use super::*;
use crate::diagram::Diagram;
use crate::doc::{Block, Doc};
use crate::glyphs::{Glyphs, Mark};
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::{visible, Description, Html, Markdown, Surface, Terminal};
use crate::width::cell_width;
use crate::words::Standing;
use closing::closing;
use on_screen::on_a_screen;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens")
}

/// What a run's decision offers, with the command that chooses each option.
fn decision() -> Decision {
    let options = vec![
        DecisionOption {
            id: "approve".to_string(),
            label: Some("builds the plan".to_string()),
            tradeoff: Some("its tasks run, a step at a time".to_string()),
            asks: None,
        },
        DecisionOption {
            id: "adjust".to_string(),
            label: Some("Back to `plan`".to_string()),
            tradeoff: Some("the plan is written again with what you say".to_string()),
            asks: Some("what should change?".to_string()),
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

/// A decision a reviewer answers on the forge that holds the pull request,
/// with the one step typed elsewhere after its options.
fn on_the_forge() -> Decision {
    let option = |id: &str, label: &str, tradeoff: &str| DecisionOption {
        id: id.to_string(),
        label: Some(label.to_string()),
        tradeoff: Some(tradeoff.to_string()),
        asks: None,
    };
    Decision {
        chosen: Chosen::OnTheForge {
            forge: "GitHub".to_string(),
            elsewhere: "on the machine that holds the run".to_string(),
        },
        options: vec![
            option("approve", "approve this pull request", "the run goes on"),
            option(
                "close",
                "close this pull request",
                "nothing runs; this decision stays open",
            ),
        ],
        otherwise: vec![(
            "yunta resume 7E5PH4".to_string(),
            "reads the answer and acts on it",
        )],
    }
}

/// The first blocks of the catalog: what a document is, what it is about,
/// and what it found.
fn opening() -> Vec<Block> {
    let mut checks = Checklist::default();
    checks.push(Found::Holds, "git", "commits as Ada <ada@example.com>");
    checks.push(Found::Caution, "forge", "github acme/web, reachable");
    checks.push(Found::NotApplicable, "review", "no fan-out review");
    let needs_you = Standing::NeedsYou.word();
    vec![
        Title {
            name: Line::new().push(Tone::Strong, "plan"),
            facts: vec!["4 tasks in 3 steps".to_string(), "2 risks".to_string()],
        }
        .into(),
        Headline::new(Mark::NeedsYou, "fragua (standard)", needs_you)
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
        Prose("Then the run builds the change and checks what it built.".to_string()).into(),
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
            fix: Fix::said("raise `limits.max_tokens_per_run`"),
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
    vec![Section {
        lead: Some(crate::components::Lead::Mark(Mark::Failed)),
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
    .into()]
    .into_iter()
    .chain(found())
    .chain([readiness().into(), named_paths()])
    .collect()
}

/// A report that folds what holds into one row, then each finding with
/// what it means and how to make it hold, the lines to paste under one.
fn readiness() -> Checklist {
    let mut checks = Checklist::default().folded("healthy");
    checks.push_check(
        Check::new(Found::Holds, "claude-code", "healthy (2.1.302)").brief("claude-code 2.1.302"),
    );
    checks.push(Found::Holds, "git", "commits as Ada <ada@example.com>");
    checks.push_check(
        Check::new(
            Found::Caution,
            "first run",
            "a new binary's first run takes 469 ms here, its next 2 ms",
        )
        .so("macOS assesses each new executable before its first run, and a suite pays that for every test binary it links again")
        .fix(Fix::said("add the terminal that runs yunta under Developer Tools, then open it again")),
    );
    checks.push_check(
        Check::new(
            Found::Problem,
            "runners",
            "none declared, and `lint-fix` needs one",
        )
        .fix(Fix {
            said: "declare a runner in .yunta/config.yaml:".to_string(),
            paste: vec![
                "runners:".to_string(),
                "  implementer:".to_string(),
                "    - { adapter: claude-code, model: <model> }".to_string(),
            ],
        }),
    );
    checks
}

/// What a check found in a file, and what a command refused: a problem
/// pointed at in its line, a warning with no place, and a refusal.
fn found() -> Vec<Block> {
    vec![
        Problem::error(
            "node `fix`: unknown key `promt` for a `prompt` node",
            Some(Place {
                path: ".yunta/workflows/lint-fix.yaml".to_string(),
                line: 9,
                col: 5,
                source: Some("    promt: \"Fix what lint reported.\"".to_string()),
                len: 5,
            }),
        )
        .help([
            "did you mean `prompt`?".to_string(),
            "`yunta schema workflow` lists every key".to_string(),
        ])
        .into(),
        Problem::warning("the program `cargo-nextest` is not on this machine's PATH").into(),
        Refusal {
            said: "no run is called `7E5PH5`; did you mean `7E5PH4`?".to_string(),
            detail: Some("looked in ~/.yunta/runs".to_string()),
        }
        .into(),
    ]
}

/// The paths a failure names, each with why it is named.
fn named_paths() -> Block {
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
    .into()
}

/// One task's card, which a reader opens for what proves the task.
fn planned() -> Vec<Block> {
    vec![Fold(Section {
        lead: None,
        title: Line::new().push(Tone::Strong, "T001").plain(": Make it"),
        blocks: vec![Prose("the file exists".to_string()).into()],
    })
    .into()]
}

/// What an author wrote, and a file's code three ways: as written, read
/// against the code it modifies, and new where nothing stands.
fn written() -> Vec<Block> {
    vec![
        Block::Markdown(
            "A change **in Markdown**, with `code`.\n\n- an item\n\n```rust\nfn main() {}\n```"
                .to_string(),
        ),
        Block::Diagram(Diagram::of("graph LR\n  plan --> build")),
        Code::whole(
            "src/lib.rs",
            Some("the change".to_string()),
            "pub fn greet() {}\n",
        )
        .into(),
        Code::whole(
            "src/greet.rs",
            Some("greets the person by name".to_string()),
            "pub fn hello(name: &str) -> String;",
        )
        .within(["hello"])
        .against(Some("pub fn hello() -> String;"))
        .into(),
        Code::whole(
            "src/greet.rs",
            Some("says goodbye".to_string()),
            "pub fn bye(name: &str) -> String;",
        )
        .within(["bye"])
        .against(None)
        .into(),
    ]
}

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
                said: format!("{} while it ran", Standing::Stopped.word()),
                edge: Edge::Command("yunta resume 60Y934".to_string()),
            },
        ],
    }
}

/// The two nodes a plan leaves: the plan itself, and the approval a person
/// gave it, whose time is a person's and so reads in the person's tone.
fn planned_rows() -> Vec<NodeRow> {
    vec![
        NodeRow {
            mark: Mark::Done,
            word: Standing::Finished.word(),
            id: "plan".to_string(),
            branch: None,
            returns: None,
            note: "4 tasks in 3 steps".to_string(),
            took: "1m31s".to_string(),
            took_by_person: false,
            spent: "1.94M".to_string(),
        },
        NodeRow {
            mark: Mark::Done,
            word: Standing::Finished.word(),
            id: "approve-plan".to_string(),
            branch: None,
            returns: None,
            note: "approved on its third round".to_string(),
            took: "6h27m".to_string(),
            took_by_person: true,
            spent: String::new(),
        },
    ]
}

/// The nodes after the plan: a parallel group's two children, which hang
/// from its branch, and a lint node whose failure returns to fix-lint.
fn fan_out_rows() -> Vec<NodeRow> {
    vec![
        NodeRow {
            mark: Mark::Absent,
            word: Standing::NeverRan.word(),
            id: "review".to_string(),
            branch: Some(Branch::Child),
            returns: None,
            note: String::new(),
            took: String::new(),
            took_by_person: false,
            spent: String::new(),
        },
        NodeRow {
            mark: Mark::Absent,
            word: Standing::NeverRan.word(),
            id: "conform".to_string(),
            branch: Some(Branch::Last),
            returns: None,
            note: String::new(),
            took: String::new(),
            took_by_person: false,
            spent: String::new(),
        },
        NodeRow {
            mark: Mark::Absent,
            word: Standing::NeverRan.word(),
            id: "lint".to_string(),
            branch: None,
            returns: Some("fix-lint".to_string()),
            note: "sends a failure back at most once".to_string(),
            took: String::new(),
            took_by_person: false,
            spent: String::new(),
        },
    ]
}

/// Every block the document is made of, each once: what the golden tests
/// and the color tests draw.
pub(super) fn catalog() -> Doc {
    opening()
        .into_iter()
        .chain(quoting())
        .chain(planned())
        .chain(written())
        .chain(closing())
        .chain(on_a_screen())
        .fold(Doc::new(), Doc::with)
}

#[test]
fn every_block_matches_its_goldens_in_every_environment() {
    let doc = catalog();
    // First, so that a color golden that fails does not keep it from being compared.
    assert_golden(&goldens().join("markdown/catalog.md"), &Markdown.draw(&doc));
    assert_golden(&goldens().join("html/catalog.html"), &Html.body(&doc));
    for environment in ENVIRONMENTS.iter().chain(&RICH) {
        let look = Look::of(environment);
        let drawn = Terminal::on(look).draw(&doc);
        for line in crate::paint::strip_sgr(&drawn).lines() {
            assert!(
                cell_width(line) <= look.width.cells(),
                "{}: past the line: {line}",
                environment.name
            );
        }
        assert_golden(
            &environment.golden(&goldens().join("components"), "catalog"),
            &drawn,
        );
    }
}

#[test]
fn markdown_says_every_word_the_terminal_says_of_every_block() {
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(120), None),
    };
    let doc = catalog();
    let terminal = words(&Terminal::on(look).draw(&doc));
    let forms = [
        ("Markdown", Markdown.draw(&doc)),
        ("Description", Description::default().draw(&doc)),
        ("HTML", visible(&Html.body(&doc))),
    ];
    for (form, drawn) in &forms {
        let said = words(drawn);
        let lost: Vec<&String> = terminal.difference(&said).collect();
        assert!(lost.is_empty(), "{form} dropped {lost:?}");
    }
}

/// A change that names no file has nothing to say, on any surface.
#[test]
fn a_change_that_names_no_file_draws_nothing() {
    let none = || {
        Doc::new().with(Diffstat {
            files: Vec::new(),
            more: None,
        })
    };
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(120), None),
    };
    assert_eq!(Terminal::on(look).draw(&none()).trim(), "");
    assert_eq!(Markdown.draw(&none()).trim(), "");
    assert!(!Html.body(&none()).contains("<table"));
}
