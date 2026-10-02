use std::path::{Path, PathBuf};

use yunta_testkit_core::golden::{assert_golden, ENVIRONMENTS};

use super::*;
use crate::doc::Doc;
use crate::ink::strip_sgr;
use crate::surface::{Markdown, Surface, Terminal};
use crate::Mark;

fn plain(block: &dyn Drawn) -> Vec<String> {
    let look = Look::plain();
    block
        .lines(&look)
        .iter()
        .map(|line| look.ink.paint(line))
        .collect()
}

/// `doc` as a terminal with `look` draws it.
fn paint(doc: &Doc<'_>, look: &Look) -> String {
    Terminal::on(*look).draw(doc)
}

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/blocks")
}

fn evidence(lines: usize) -> Evidence {
    Evidence {
        tail: (1..=lines).map(|n| format!("line {n}")).collect(),
        whole: Some(Whole::File {
            shown: "~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4/objects/9f2c".to_string(),
            path: PathBuf::from("/home/me/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4/objects/9f2c"),
        }),
    }
}

fn decision() -> Decision {
    Decision {
        handle: "7E5PH4".to_string(),
        options: vec![
            DecisionOption {
                id: "retry".to_string(),
                label: Some("runs `fix-lint` once more".to_string()),
                tradeoff: "one more correction attempt beyond `max_reroutes`".to_string(),
                asks: None,
            },
            DecisionOption {
                id: "adjust".to_string(),
                label: None,
                tradeoff: "the run goes on with what you say".to_string(),
                asks: Some("what to change".to_string()),
            },
        ],
    }
}

#[test]
fn a_field_with_nothing_to_say_is_not_drawn() {
    let fields = Fields::new()
        .push_if("tokens", "20.2k spent")
        .push_if("branch", "")
        .push_if("artifacts", "  ");
    assert_eq!(plain(&fields), ["  tokens       20.2k spent"]);
}

#[test]
fn a_long_value_hangs_under_its_first_line() {
    let fields = Fields::new().push_if("branch", "word ".repeat(30));
    let drawn = plain(&fields);
    assert!(drawn.len() > 1, "{drawn:?}");
    let column = "  branch       ".len();
    for line in drawn.iter().skip(1) {
        assert_eq!(line.find("word"), Some(column), "{drawn:?}");
    }
}

#[test]
fn evidence_shows_at_most_six_lines_and_says_where_the_rest_is() {
    let drawn = plain(&evidence(20));
    let quoted: Vec<&String> = drawn
        .iter()
        .filter(|line| line.starts_with("  | "))
        .collect();
    assert_eq!(quoted.len(), 6, "{drawn:?}");
    assert_eq!(quoted[0], "  | line 15");
    assert_eq!(quoted[5], "  | line 20");
    let rest = drawn[6..].join("\n");
    assert!(
        rest.contains("14 lines above") && rest.contains("whole output: ~/.yunta/runs/"),
        "{drawn:?}"
    );

    let short = plain(&Evidence {
        tail: vec!["exit 1".to_string()],
        whole: None,
    });
    assert_eq!(
        short,
        ["  | exit 1"],
        "nothing more to say says nothing more"
    );
}

#[test]
fn a_decision_prints_one_command_per_option_with_the_handle() {
    let drawn = plain(&decision());
    let commands: Vec<&String> = drawn
        .iter()
        .filter(|line| line.contains("yunta resolve-gate"))
        .collect();
    assert_eq!(
        commands,
        [
            "    yunta resolve-gate 7E5PH4 retry",
            "    yunta resolve-gate 7E5PH4 adjust --text \"<answer>\""
        ]
    );
    assert!(
        drawn
            .iter()
            .any(|line| line.contains("asks: what to change")),
        "{drawn:?}"
    );
}

#[test]
fn a_node_id_keeps_its_column_and_a_note_is_cut_to_the_line() {
    let table = NodeTable {
        rows: vec![
            NodeRow {
                mark: Mark::Failed,
                word: "failed",
                id: "review@claude-code".to_string(),
                note: "exit 101 ".repeat(20),
            },
            NodeRow {
                mark: Mark::Done,
                word: "finished",
                id: "lint".to_string(),
                note: String::new(),
            },
        ],
    };
    let drawn = plain(&table);
    assert!(
        drawn
            .iter()
            .all(|line| crate::cell_width(line) <= Look::plain().width.cells()),
        "{drawn:?}"
    );
    assert_eq!(drawn[1].trim_end(), "  + finished  lint");
}

#[test]
fn every_block_reads_the_same_once_its_color_is_taken_out() {
    let look = Look::of(&ENVIRONMENTS[0]);
    let flat = Look {
        ink: crate::ink::Ink::Plain,
        ..look
    };
    let linked = Look {
        ink: crate::ink::Ink::Linked,
        ..look
    };
    let blocks = Doc::new().with(decision()).with(evidence(3));
    assert_eq!(strip_sgr(&paint(&blocks, &look)), paint(&blocks, &flat));
    assert_eq!(strip_sgr(&paint(&blocks, &linked)), paint(&blocks, &flat));
}

#[test]
fn a_path_is_a_link_only_when_links_are_on() {
    let painted = |ink| {
        let look = Look {
            ink,
            ..Look::of(&ENVIRONMENTS[0])
        };
        paint(&Doc::new().with(evidence(3)), &look)
    };
    let target =
        "\x1b]8;;file:///home/me/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4/objects/9f2c\x1b\\";
    assert!(
        painted(crate::ink::Ink::Linked).contains(target),
        "a terminal that opens links gets the file as one"
    );
    for ink in [crate::ink::Ink::Ansi16, crate::ink::Ink::Plain] {
        assert!(!painted(ink).contains("\x1b]8"), "{ink:?}");
    }
}

/// Every block with something to say, as one surface would put them.
fn every_block() -> Doc<'static> {
    let headline = Headline {
        subject: "run 7E5PH4".to_string(),
        mark: Mark::NeedsYou,
        said: "needs you on a decision".to_string(),
    };
    let fields = Fields::new()
        .push_if("progress", "nodes 1/2 · 1 failed")
        .push_if("tokens", "20.2k spent · 3 past runs, median 18k tokens");
    let table = NodeTable {
        rows: vec![
            NodeRow {
                mark: Mark::Failed,
                word: "failed",
                id: "lint".to_string(),
                note: "exit 101".to_string(),
            },
            NodeRow {
                mark: Mark::Pending,
                word: "never ran",
                id: "fix-lint".to_string(),
                note: String::new(),
            },
        ],
    };
    let next = Next {
        steps: vec![
            ("yunta status 7E5PH4".to_string(), "where it stands"),
            ("yunta close 7E5PH4".to_string(), "closes it for good"),
        ],
    };
    let mut printed = evidence(8);
    printed.tail.push(
        "error[E0425]: cannot find value `undefined_variable_name` in this scope, \
         reported at src/lib.rs:3:5 by the compiler"
            .to_string(),
    );
    Doc::new()
        .with(headline)
        .with(table)
        .with(printed)
        .with(decision())
        .with(fields)
        .with(next)
}

#[test]
fn blocks_match_their_goldens() {
    let blocks = every_block();
    for environment in &ENVIRONMENTS {
        let look = Look::of(environment);
        assert_golden(
            &environment.golden(&goldens(), "every-block"),
            &paint(&blocks, &look),
        );
    }
}

#[test]
fn markdown_matches_its_golden() {
    assert_golden(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/markdown/every-block.md"),
        &Markdown.draw(&every_block()),
    );
}

/// The words of `text` once markup, glyphs and the padding of columns
/// are gone: what carries the meaning, whichever medium drew it.
fn words(text: &str) -> std::collections::BTreeSet<String> {
    text.split_whitespace()
        .map(|token| token.trim_matches(|c| matches!(c, '*' | '`' | '|' | ':')))
        .filter(|token| token.chars().any(char::is_alphanumeric))
        .map(str::to_string)
        .collect()
}

#[test]
fn markdown_says_every_word_the_terminal_does() {
    let look = Look {
        glyphs: crate::Glyphs::Unicode,
        ink: crate::ink::Ink::Plain,
        width: crate::Width::of(Some(120), None),
    };
    let terminal = words(&paint(&every_block(), &look));
    let markdown = words(&Markdown.draw(&every_block()));
    let lost: Vec<&String> = terminal.difference(&markdown).collect();
    assert!(lost.is_empty(), "Markdown dropped {lost:?}");
}

#[test]
fn a_quoted_refusal_matches_its_golden() {
    let text = "name: fix\nnodes:\n  - id: lint\n    kind: bash\n    run: { command: lnt }\n  - id: fix\n    kind: prompt\n    prompt: p\n    runner: implementr\n";
    let at = |line, col, len| Some(yunta_core::yaml::Location { line, col, len });
    let problems = [
        (
            "node `lint`: it runs the project's command `lnt`, and the config declares none — \
             did you mean `lint`?"
                .to_string(),
            at(5, 21, 3),
        ),
        (
            "node `fix` references runner `implementr`, which `runners:` does not define — \
             did you mean `implementer`?"
                .to_string(),
            at(9, 13, 10),
        ),
        ("a conflict between config layers".to_string(), None),
    ];
    assert_golden(
        &goldens().join("quoted-refusal.txt"),
        &super::diagnostic::located("wf.yaml", &problems, "wf.yaml", Some(text)),
    );
}

#[test]
fn a_checklist_marks_each_check_and_lines_up_what_it_found() {
    let mut list = Checklist::default();
    list.push(Found::Holds, "git", "commits as Ada <ada@example.com>");
    list.push(Found::Caution, "forge", "github acme/web — reachable");
    list.push(
        Found::Problem,
        "codex/codex-model (executor, planner fallback, reviewer)",
        "session died",
    );
    assert_eq!(
        plain(&list),
        [
            "  + git    commits as Ada <ada@example.com>",
            "  ! forge  github acme/web — reachable",
            "  x codex/codex-model (executor, planner fallback, reviewer)",
            "           session died",
        ]
    );
    assert!(!list.holds(), "a problem on the list is one a run stops on");
}
