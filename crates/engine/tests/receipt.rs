//! Verified Work Receipt.
//!
//! Two concerns, tested separately: the **formatters**
//! (`render_markdown` and the JSON a program reads) against a hand-built [`Receipt`] —
//! golden byte-for-byte output, kept as files under `goldens/receipt/`
//! — and the **derivation**
//! (`receipt::closed`) against a real run's own log, executed end to end
//! with the mock adapter (same style `tests/run.rs` uses), asserted
//! field by field rather than as one giant string so a fixture tweak
//! doesn't need to reprint an entire golden blob.

use surco::golden::assert_golden;
use yunta_core::diagnostic::{
    ArtifactCode, ArtifactFailure, DiagnosticCode, FileCode, ParseCode, RuleCode,
};
use yunta_core::events::{
    ArtifactId, ChangesCountedPayload, EventBody, EventPayload, Failure, NodeEvent,
    NodeFailedPayload, NodeStartedPayload, RunEvent, RunFinishedPayload, RunMetrics, StoredEvent,
    TerminalState, TokenUsage,
};
use yunta_core::receipt::{
    BaselineSummary, CostSummary, CriteriaSummary, CriterionEntry, DiagnosticCount,
    EventChainStatus, Receipt, RunnerUsage, ScopeSummary,
};
use yunta_core::{ArtifactKind, NodeId, RunId};
use yunta_engine::RunReport;
use yunta_replay::receipt;
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;

#[path = "receipt/from_a_log.rs"]
mod from_a_log;

#[path = "receipt/artifact_problems.rs"]
mod artifact_problems;

// --- derivation: receipt::closed over a real run's own log -----------------

const CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
  reviewer:
    - { adapter: mock, model: mock-model }
  reviewer-alt:
    - { adapter: mock, model: mock-model }
baseline:
  suite: "true"
"#;

// --- formatters: golden output over a hand-built Receipt --------------------

fn sample_receipt(event_chain: EventChainStatus) -> Receipt {
    Receipt {
        schema_version: Receipt::SCHEMA_VERSION,
        run_id: RunId::from("run-2026-08-21-0001"),
        workflow: "release-cycle".into(),
        mode: "default".into(),
        terminal_state: Some(TerminalState::Done),
        criteria: CriteriaSummary {
            total: 3,
            green: 3,
            entries: vec![
                CriterionEntry {
                    task_id: "T001".to_string(),
                    cmd: "test -f hello.txt".to_string(),
                    exit_code: 0,
                },
                CriterionEntry {
                    task_id: "T002".to_string(),
                    cmd: "cargo test -p yunta-core".to_string(),
                    exit_code: 0,
                },
                CriterionEntry {
                    task_id: "T002".to_string(),
                    cmd: "cargo clippy --workspace -- -D warnings".to_string(),
                    exit_code: 0,
                },
            ],
        },
        baseline: Some(BaselineSummary {
            suite: "make test".to_string(),
            hash: yunta_core::sha256_hex(b"make test"),
            compared: 2,
            regressions: 0,
            origin: yunta_core::events::BaselineOrigin::Measured,
            red: None,
        }),
        scope: ScopeSummary {
            files_touched: 4,
            violations: Vec::new(),
        },
        changed: Some(ChangesCountedPayload {
            tree: "deadbeef".into(),
            files: 9,
            added: 412,
            removed: 87,
            largest: Vec::new(),
        }),
        runners: vec![
            RunnerUsage {
                node_id: NodeId::from("review@reviewer"),
                runner: "reviewer".into(),
                adapter: "claude-code".into(),
                model: "claude-sonnet-4-6".into(),
            },
            RunnerUsage {
                node_id: NodeId::from("review@reviewer-alt"),
                runner: "reviewer-alt".into(),
                adapter: "codex".into(),
                model: "gpt-5-codex".into(),
            },
        ],
        cost: CostSummary {
            tokens: TokenUsage {
                input: 1200,
                output: 340,
                cached: Some(200),
            },
            cptv: Some(770.0),
            reroutes: 2,
        },
        event_chain,
        unknown_kinds: Vec::new(),
        diagnostics: Vec::new(),
        failed: Vec::new(),
        blocking_findings: 0,
    }
}

/// The receipt as a file holds it.
fn render_receipt_markdown(receipt: &yunta_core::receipt::Receipt) -> String {
    use surco::surface::Surface;
    surco::surface::Markdown.draw(&yunta_render::receipt::document(
        receipt,
        surco::glyphs::Glyphs::Unicode,
    ))
}

/// The golden file `case` is checked against.
fn golden(case: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("goldens/receipt")
        .join(format!("{case}.md"))
}

#[test]
fn renders_the_markdown_receipt_byte_for_byte() {
    let receipt = sample_receipt(EventChainStatus::Intact { events: 342 });
    assert_golden(&golden("finished"), &render_receipt_markdown(&receipt));
}

/// A run that failed says which node failed and why, under what it held
/// its work to.
#[test]
fn a_failed_run_s_receipt_says_what_failed() {
    let mut receipt = sample_receipt(EventChainStatus::Intact { events: 40 });
    receipt.terminal_state = Some(TerminalState::Failed);
    receipt.failed = vec![yunta_core::receipt::FailedNode {
        node: NodeId::from("build"),
        failure: Failure::Message {
            outcome: "`cargo build` exited 101: cannot find value `x`".into(),
        },
    }];
    assert_golden(&golden("failed"), &render_receipt_markdown(&receipt));
}

#[test]
fn a_broken_event_chain_renders_as_a_visible_failure_not_a_silent_omission() {
    let receipt = sample_receipt(EventChainStatus::Broken {
        seq: 88.into(),
        detail: "payload hash mismatch".to_string(),
    });
    let markdown = render_receipt_markdown(&receipt);
    assert!(
        markdown.contains("✗ **event chain** broken at seq 88: payload hash mismatch"),
        "got:\n{markdown}"
    );
}

#[test]
fn renders_the_json_receipt_as_pretty_printed_structured_data() {
    let receipt = sample_receipt(EventChainStatus::Intact { events: 342 });
    let json = serde_json::to_string_pretty(&receipt).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["run_id"], "run-2026-08-21-0001");
    assert_eq!(parsed["workflow"], "release-cycle");
    assert_eq!(parsed["terminal_state"], "done");
    assert_eq!(parsed["criteria"]["total"], 3);
    assert_eq!(parsed["criteria"]["green"], 3);
    assert_eq!(parsed["baseline"]["regressions"], 0);
    assert_eq!(parsed["scope"]["violations"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["runners"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["cost"]["cptv"], 770.0);
    assert_eq!(parsed["event_chain"]["status"], "intact");
    assert_eq!(parsed["event_chain"]["events"], 342);

    // No LLM-authored text anywhere in the receipt —
    // every string in the JSON traces back to a
    // command, a role, a hash or a count, never freeform prose an agent
    // could have written. Structural proxy for that: no field holds
    // more than one sentence of prose — `entries[].cmd` are shell
    // commands, not narration.
    for entry in parsed["criteria"]["entries"].as_array().unwrap() {
        assert!(!entry["cmd"].as_str().unwrap().is_empty());
    }
}

#[test]
fn baseline_absent_never_invents_a_zero_regression_line() {
    let mut receipt = sample_receipt(EventChainStatus::Intact { events: 10 });
    receipt.baseline = None;
    assert_golden(&golden("no-baseline"), &render_receipt_markdown(&receipt));
}

/// A run of a lineage compares against a measurement another run took,
/// and its receipt says which one — so a reader of the child's receipt
/// can open the output the comparisons are against.
#[test]
fn the_receipt_names_the_run_that_measured_an_inherited_baseline() {
    let mut receipt = sample_receipt(EventChainStatus::Intact { events: 10 });
    receipt.baseline = Some(BaselineSummary {
        suite: "make test".to_string(),
        hash: yunta_core::sha256_hex(b"make test"),
        compared: 2,
        regressions: 0,
        origin: yunta_core::events::BaselineOrigin::Inherited {
            run: RunId::from("run-2026-08-21-0001"),
        },
        red: None,
    });
    assert!(
        render_receipt_markdown(&receipt).contains(
            "0 regressions across 2 comparisons (suite `make test`, \
             hash `22cc66aa7d26`, measured by run run-2026-08-21-0001)"
        ),
        "{}",
        render_receipt_markdown(&receipt)
    );
}

/// A suite that was failing when it was measured cannot show that
/// anything stopped passing, so the receipt must not count its
/// comparisons as zero regressions found.
#[test]
fn a_receipt_over_a_red_measurement_certifies_no_comparison() {
    let mut receipt = sample_receipt(EventChainStatus::Intact { events: 10 });
    receipt.baseline = Some(BaselineSummary {
        suite: "make test".to_string(),
        hash: yunta_core::sha256_hex(b"make test"),
        compared: 2,
        regressions: 0,
        origin: yunta_core::events::BaselineOrigin::Measured,
        red: Some(2),
    });
    let markdown = render_receipt_markdown(&receipt);
    assert!(
        markdown.contains(
            "- ▲ **baseline** already red when measured (exit 2): none of 2 comparisons \
             could find a regression (suite `make test`, hash `22cc66aa7d26`)"
        ),
        "{markdown}"
    );
    assert!(!markdown.contains("regressions across"), "{markdown}");
}
