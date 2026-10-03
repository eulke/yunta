//! A finding's proposed criterion is evidence only when it fails before
//! the fix: one that already passes on the run's tree, or that cannot
//! run, is refused as the finding is reported. Answered fixed, it runs on
//! the tree the answering node leaves, and passing settles the finding:
//! it stands, and no longer keeps the run from passing.

mod common;

use yunta_core::events::{EventPayload, FindingEvent};
use yunta_testkit::Bench;

const REVIEWS: &str = r#"
name: reviewed
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the change."
    artifacts:
      produces: [findings]
"#;

/// A review that reports one finding proposing `cmd`, refused unless
/// `accepted`.
fn reporting(cmd: &str, accepted: bool) -> String {
    let expect = if accepted {
        ""
    } else {
        "        expect: refused\n"
    };
    format!(
        "capabilities: {{ run_tools: true }}
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
{expect}        arguments:
          id: missing
          severity: blocking
          title: \"the file is missing\"
          location: \"fixed.txt\"
          detail: \"nothing writes it\"
          proposed_criterion: {{ cmd: {cmd:?} }}
    outcome: {{ type: completed, summary: reviewed }}
"
    )
}

/// The codes of every finding call the run refused.
fn refused(bench: &Bench) -> Vec<String> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Findings(FindingEvent::Refused(p))) => Some(
                p.report
                    .diagnostics
                    .iter()
                    .map(|d| d.code().as_str().to_string())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect()
}

#[tokio::test]
async fn a_proposed_criterion_that_already_passes_is_refused() {
    let bench = Bench::new();
    bench.run(REVIEWS, &reporting("true", false)).await;

    assert_eq!(refused(&bench), ["proposed-criterion-already-passes"]);
}

#[tokio::test]
async fn a_proposed_criterion_that_fails_on_the_tree_is_taken() {
    let bench = Bench::new();
    bench
        .run(REVIEWS, &reporting("test -f fixed.txt", true))
        .await;

    assert!(refused(&bench).is_empty(), "{:?}", refused(&bench));
}

#[tokio::test]
async fn a_proposed_criterion_that_cannot_run_is_refused() {
    let bench = Bench::new();
    bench
        .run(REVIEWS, &reporting("yunta-no-such-tool --check", false))
        .await;

    assert_eq!(refused(&bench), ["criterion-cannot-run"]);
}

/// A review that finds `fixed.txt` missing, a fix that answers it, and a
/// gate on what is left blocking.
const PROVED: &str = r#"
name: proved
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the change."
    artifacts:
      produces: [findings]
  - id: fix
    kind: prompt
    runner: executor
    depends_on: [review]
    prompt: "Fix the findings."
  - id: held
    kind: check
    builtin: findings_gate
    max_severity: blocking
    depends_on: [fix]
"#;

/// The review's finding, and a fix that writes `effects` and answers it
/// `answer`.
fn fixing(effects: &str, answer: &str) -> String {
    format!(
        "capabilities: {{ run_tools: true }}
sessions:
  - match_prompt_contains: \"Review the change\"
    steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: missing
          severity: blocking
          title: \"the file is missing\"
          location: \"fixed.txt\"
          detail: \"nothing writes it\"
          proposed_criterion: {{ cmd: \"test -f fixed.txt\" }}
    outcome: {{ type: completed, summary: reviewed }}
  - match_prompt_contains: \"Fix the findings\"
    effects:
{effects}    steps:
      - type: run_tool
        tool: yunta_answer_finding
        arguments: {{ node: review, id: missing, answer: {answer}, why: \"see the file\" }}
    outcome: {{ type: completed, summary: answered }}
"
    )
}

/// Every proof on the log: the command and its exit.
fn proofs(bench: &Bench) -> Vec<(String, i32)> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Findings(FindingEvent::Proved(p))) => {
                Some((p.result.cmd.clone(), p.result.exit_code))
            }
            _ => None,
        })
        .collect()
}

fn blocking(bench: &Bench, workflow: &str) -> usize {
    let workflow: yunta_core::Workflow = yunta_core::yaml::parse(workflow).unwrap();
    let events = bench.events();
    let at = events.last().unwrap().timestamp;
    yunta_engine::run_frame(&bench.run_id, &workflow, &events, None, at).blocking_findings
}

const WRITES_IT: &str = "      - { path: fixed.txt, content: fixed }\n";

#[tokio::test]
async fn a_proved_blocking_finding_lets_the_run_succeed() {
    let bench = Bench::new();
    let yunta_engine::RunReport { terminal, .. } =
        bench.run(PROVED, &fixing(WRITES_IT, "fixed")).await;

    assert_eq!(terminal, yunta_engine::RunTerminal::Finished);
    assert_eq!(proofs(&bench), [("test -f fixed.txt".to_string(), 0)]);
    assert_eq!(
        blocking(&bench, PROVED),
        0,
        "settled, it stands and no longer counts"
    );
}

#[tokio::test]
async fn a_failing_criterion_is_recorded_and_settles_nothing() {
    let bench = Bench::new();
    let yunta_engine::RunReport { terminal, .. } = bench.run(PROVED, &fixing("", "fixed")).await;

    assert!(
        matches!(terminal, yunta_engine::RunTerminal::Paused { .. }),
        "the gate on what is left blocking holds: {terminal:?}"
    );
    assert_eq!(proofs(&bench), [("test -f fixed.txt".to_string(), 1)]);
    assert_eq!(blocking(&bench, PROVED), 1);
}

#[tokio::test]
async fn a_declined_finding_is_not_proved() {
    let bench = Bench::new();
    bench.run(PROVED, &fixing(WRITES_IT, "declined")).await;

    assert!(proofs(&bench).is_empty(), "{:?}", proofs(&bench));
    assert_eq!(blocking(&bench, PROVED), 1, "a word is not evidence");
}
