//! What a session writes is formatted the moment it writes it, by the
//! command the project's `format:` rule names for that file — and the
//! session hears of it only when something changed or the command failed.

use yunta_core::events::{EventPayload, SessionEvent, WriteFormattedPayload};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, Bench};

/// Uppercases every file it is handed; exits 1 on one named `bad.txt`.
const UPPER: &str = "for f in \"$@\"; do\n  [ \"$f\" = bad.txt ] && { echo 'bad.txt: cannot format' >&2; exit 1; }\n  tr a-z A-Z < \"$f\" > \"$f.up\" && mv \"$f.up\" \"$f\"\ndone\n";

const CONFIG: &str = "\
runners:
  executor:
    - { adapter: mock, model: mock-model }
commands:
  upper: \"sh upper.sh\"
format:
  - { paths: [\"*.txt\"], run: upper }
";

const WORKFLOW: &str =
    "name: w\nnodes:\n  - { id: write, kind: prompt, runner: executor, prompt: \"Write.\" }\n";

/// Every format the run recorded, in order.
fn formatted(bench: &Bench) -> Vec<WriteFormattedPayload> {
    bench
        .events()
        .into_iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::WriteFormatted(formatted))) => {
                Some(formatted.clone())
            }
            _ => None,
        })
        .collect()
}

async fn bench() -> Bench {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join("upper.sh"), UPPER)
        .await
        .unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "formatter"]);
    bench
}

#[tokio::test]
async fn a_written_file_a_rule_covers_is_formatted_and_one_no_rule_covers_is_left_alone() {
    let bench = bench().await;
    let fixture = "capabilities: { run_tools: true, fence: tool_calls }\neffects:\n  - { path: a.txt, content: \"hello\" }\n  - { path: b.md, content: \"hello\" }\n  - { path: c.txt, content: \"ALREADY\" }\noutcome: { type: completed, summary: wrote }\n";

    let RunReport { terminal, .. } = bench.run_with_config(WORKFLOW, fixture, CONFIG).await;

    assert_eq!(terminal, RunTerminal::Finished);
    let formatted = formatted(&bench);
    let seen: Vec<(Vec<String>, Vec<String>, i32)> = formatted
        .iter()
        .map(|f| {
            let names = |paths: &[std::path::PathBuf]| {
                paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
            };
            (names(&f.paths), names(&f.changed), f.exit_code)
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            (vec!["a.txt".to_string()], vec!["a.txt".to_string()], 0),
            (vec!["c.txt".to_string()], vec![], 0),
        ],
        "`b.md` is no rule's; `c.txt` was already as the rule leaves it"
    );
    assert!(formatted.iter().all(|f| f.run.as_str() == "upper"));
}

#[tokio::test]
async fn a_formatter_that_fails_is_recorded_with_its_exit_and_the_session_goes_on() {
    let bench = bench().await;
    let fixture = "capabilities: { run_tools: true, fence: tool_calls }\neffects:\n  - { path: bad.txt, content: \"x\" }\noutcome: { type: completed, summary: wrote }\n";

    let RunReport { terminal, .. } = bench.run_with_config(WORKFLOW, fixture, CONFIG).await;

    assert_eq!(terminal, RunTerminal::Finished);
    let formatted = formatted(&bench);
    assert_eq!(formatted.len(), 1);
    assert_eq!(formatted[0].exit_code, 1);
    assert!(formatted[0].changed.is_empty());
}

#[tokio::test]
async fn a_run_without_format_rules_formats_nothing() {
    let bench = bench().await;
    let fixture = "capabilities: { run_tools: true, fence: tool_calls }\neffects:\n  - { path: a.txt, content: \"hello\" }\noutcome: { type: completed, summary: wrote }\n";
    let config = "runners:\n  executor:\n    - { adapter: mock, model: mock-model }\n";

    let RunReport { terminal, .. } = bench.run_with_config(WORKFLOW, fixture, config).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(formatted(&bench).is_empty());
    assert!(
        !bench.mock().requests_seen()[0].formats,
        "no hook after a write is installed"
    );
}
