//! A session the token cap stops as it runs. An adapter that counts the cap
//! mid-session ends the session at the usage report that passes the cap: the
//! reports after it are never counted, and the completion is never read.

use super::*;

/// The scripted adapter, declaring `mid_session_token_cap` so the cap is
/// checked at every usage report rather than once the session closes.
fn scripted_counting_the_cap(yaml: &str) -> MockAdapter {
    MockAdapter::from_yaml(yaml)
        .unwrap()
        .narrowed(Capabilities {
            mid_session_token_cap: true,
            ..Capabilities::default()
        })
}

#[tokio::test]
async fn a_session_past_its_token_cap_stops_at_the_usage_report_that_passes_it() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let t = task(
        "mid-stream-cap",
        &["marker.txt"],
        vec![cmd("test -f marker.txt")],
    );
    // The first report alone totals 110, past the cap of 100. The total
    // the stop names is that report's: a stop at the close would name 150.
    let adapter = scripted_counting_the_cap(
        r#"
dialect: claude-code
steps:
  - { type: usage, input_tokens: 60, output_tokens: 50 }
  - { type: usage, input_tokens: 20, output_tokens: 20 }
outcome: { type: completed, summary: "should never be reached" }
"#,
    );
    let budget = Budget {
        max_tokens: Some(100),
        ..Default::default()
    };

    let report = run_once(&owner, &unit, run.path(), &t, &adapter, budget).await;

    match &report.attempts[0].dispatch {
        DispatchOutcome::BudgetExceeded { reason } => {
            assert_eq!(reason, "exceeded max_tokens 100 (110 used)")
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}
