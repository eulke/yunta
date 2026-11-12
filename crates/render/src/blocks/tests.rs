use std::path::{Path, PathBuf};

use surco::components::DecisionOption;
use surco::golden::assert_golden;
use surco::look::Look;
use yunta_core::RunId;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/blocks")
}

#[test]
fn a_decision_prints_one_command_per_option_with_the_handle() {
    let options = vec![
        DecisionOption {
            id: "retry".to_string(),
            label: Some("runs `fix-lint` once more".to_string()),
            tradeoff: Some("one more correction attempt beyond `max_reroutes`".to_string()),
            asks: None,
        },
        DecisionOption {
            id: "adjust".to_string(),
            label: None,
            tradeoff: Some("the run goes on with what you say".to_string()),
            asks: Some("what to change".to_string()),
        },
    ];
    let decision = super::decision::here(&RunId::from_static("7E5PH4"), options);
    let look = Look::plain();
    let drawn: Vec<String> = surco::surface::Terminal::on(look)
        .block(&decision.into())
        .iter()
        .map(|line| look.paint.line(line))
        .collect();
    let commands: Vec<&String> = drawn
        .iter()
        .filter(|line| line.contains("yunta resolve-gate"))
        .collect();
    assert_eq!(
        commands,
        [
            "|   yunta resolve-gate 7E5PH4 retry",
            "|   yunta resolve-gate 7E5PH4 adjust --text \"<answer>\""
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
fn a_quoted_refusal_matches_its_golden() {
    let text = "name: fix\nnodes:\n  - id: lint\n    kind: bash\n    run: { command: lnt }\n  - id: fix\n    kind: prompt\n    prompt: p\n    runner: implementr\n";
    let at = |line, col, len| Some(yunta_core::yaml::Location { line, col, len });
    let problems = vec![
        (
            "node `lint`: it runs the project's command `lnt`, and the config declares none",
            at(5, 21, 3),
            Some("lint"),
        ),
        (
            "node `fix` references runner `implementr`, which `runners:` does not define",
            at(9, 13, 10),
            Some("implementer"),
        ),
        ("a conflict between config layers", None, None),
    ];
    assert_golden(
        &goldens().join("quoted-refusal.txt"),
        &super::diagnostic::plain(
            "wf.yaml",
            problems
                .into_iter()
                .map(|(said, at, near)| {
                    super::diagnostic::located(said.to_string(), at, "wf.yaml", Some(text))
                        .help(near.map(surco::text::suggested))
                })
                .collect(),
        ),
    );
}
