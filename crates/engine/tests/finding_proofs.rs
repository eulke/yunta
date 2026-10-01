//! A finding's proposed criterion is evidence only when it fails before
//! the fix: one that already passes on the run's tree, or that cannot
//! run, is refused as the finding is reported.

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
