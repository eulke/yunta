use std::path::{Path, PathBuf};

use yunta_testkit::{assert_golden, ENVIRONMENTS};

use super::*;
use crate::render::ink::strip_sgr;
use crate::render::Mark;

fn plain(block: &dyn Block) -> Vec<String> {
    let look = Look::plain();
    paint(&[block], &look).lines().map(str::to_string).collect()
}

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/blocks")
}

fn evidence(lines: usize) -> Evidence {
    Evidence {
        tail: (1..=lines).map(|n| format!("line {n}")).collect(),
        whole: Some("~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4/objects/9f2c".to_string()),
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
            .all(|line| crate::render::cell_width(line) <= Look::plain().width.cells()),
        "{drawn:?}"
    );
    assert_eq!(drawn[1].trim_end(), "  + finished  lint");
}

#[test]
fn every_block_reads_the_same_once_its_color_is_taken_out() {
    let look = Look::of(&ENVIRONMENTS[0]);
    let flat = Look {
        ink: crate::render::ink::Ink::Plain,
        ..look
    };
    let blocks: [&dyn Block; 2] = [&decision(), &evidence(3)];
    assert_eq!(strip_sgr(&paint(&blocks, &look)), paint(&blocks, &flat));
}

#[test]
fn blocks_match_their_goldens() {
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
    let blocks: [&dyn Block; 6] = [&headline, &table, &printed, &decision(), &fields, &next];
    for environment in &ENVIRONMENTS {
        let look = Look::of(environment);
        assert_golden(
            &environment.golden(&goldens(), "every-block"),
            &paint(&blocks, &look),
        );
    }
}
