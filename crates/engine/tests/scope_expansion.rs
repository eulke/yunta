//! `scope_expansion::evaluate` — the engine's own
//! decision over an agent's expansion request, exercised directly against
//! a real git worktree (the pre-check and the `rules`-mode size bound
//! both need real `git diff`).

use yunta_core::ProposedCriterionEntry;
use yunta_core::ScopeExpansionMode;
use yunta_engine::process::Supervision;
use yunta_engine::scope_expansion::{evaluate, Decision, GrantLedger, ScopeExpansionRequest};
use yunta_testkit::init_repo;

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    dir
}

fn request(paths: &[&str], criterion: Option<&str>) -> ScopeExpansionRequest {
    ScopeExpansionRequest {
        paths: paths.iter().map(|s| s.to_string()).collect(),
        reason: "small adjacent fix".to_string(),
        proposed_criterion: criterion.map(|cmd| ProposedCriterionEntry {
            cmd: cmd.to_string(),
        }),
    }
}

#[tokio::test]
async fn a_proposed_criterion_that_already_passes_is_denied_without_consulting_any_mode() {
    // ✓ del Plan: "criterio propuesto que ya pasa → rechazo automático sin
    // consultar" — cierto incluso en `ask`, que de otro modo escalaría.
    let dir = repo();
    let req = request(&["src/x.rs"], Some("true"));
    let (precheck, decision) = evaluate(
        ScopeExpansionMode::Ask,
        &[],
        None,
        5,
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    assert_eq!(precheck, Some(0));
    match decision {
        Decision::Denied(reason) => assert_eq!(
            reason,
            "proposed criterion already passes — nothing to expand for"
        ),
        other => panic!("expected Denied, got {other:?}"),
    }
}

#[tokio::test]
async fn deny_mode_denies_without_running_any_rule() {
    let dir = repo();
    let req = request(&["src/x.rs"], None);
    let (_precheck, decision) = evaluate(
        ScopeExpansionMode::Deny,
        &[],
        None,
        5,
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    match decision {
        Decision::Denied(reason) => {
            assert_eq!(reason, "scope_expansion mode is deny (the default)")
        }
        other => panic!("expected Denied, got {other:?}"),
    }
}

#[tokio::test]
async fn ask_mode_escalates_instead_of_deciding() {
    let dir = repo();
    let req = request(&["src/x.rs"], Some("false"));
    let (_precheck, decision) = evaluate(
        ScopeExpansionMode::Ask,
        &[],
        None,
        5,
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    assert_eq!(decision, Decision::Escalate);
}

#[tokio::test]
async fn rules_mode_grants_a_small_in_bounds_request_with_a_red_criterion() {
    let dir = repo();
    std::fs::write(dir.path().join("src.rs"), "small change").unwrap();
    let req = request(&["src.rs"], Some("test -f nonexistent-marker"));
    let (precheck, decision) = evaluate(
        ScopeExpansionMode::Rules,
        &["src.rs".to_string()],
        None,
        5,
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    assert_eq!(precheck, Some(1), "the criterion must be genuinely red");
    assert_eq!(decision, Decision::Granted);
}

#[tokio::test]
async fn rules_mode_denies_a_path_outside_within() {
    let dir = repo();
    std::fs::write(dir.path().join("outside.rs"), "x").unwrap();
    let req = request(&["outside.rs"], Some("test -f nonexistent-marker"));
    let (_precheck, decision) = evaluate(
        ScopeExpansionMode::Rules,
        &["src/**".to_string()],
        None,
        5,
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    match decision {
        Decision::Denied(reason) => assert_eq!(
            reason,
            "requested path(s) fall outside the declared `within` ceiling: [\"outside.rs\"]"
        ),
        other => panic!("expected Denied, got {other:?}"),
    }
}

#[tokio::test]
async fn rules_mode_requires_a_proposed_criterion() {
    let dir = repo();
    std::fs::write(dir.path().join("src.rs"), "x").unwrap();
    let req = request(&["src.rs"], None);
    let (_precheck, decision) = evaluate(
        ScopeExpansionMode::Rules,
        &["src.rs".to_string()],
        None,
        5,
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    match decision {
        Decision::Denied(reason) => {
            assert_eq!(reason, "rules mode requires a proposed_criterion")
        }
        other => panic!("expected Denied, got {other:?}"),
    }
}

#[tokio::test]
async fn rules_mode_denies_a_request_touching_too_many_files() {
    let dir = repo();
    for n in 0..10 {
        std::fs::write(dir.path().join(format!("f{n}.rs")), "x").unwrap();
    }
    let req = request(&["f*.rs"], Some("test -f nonexistent-marker"));
    let (_precheck, decision) = evaluate(
        ScopeExpansionMode::Rules,
        &["f*.rs".to_string()],
        None,
        5,
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    match decision {
        Decision::Denied(reason) => assert_eq!(
            reason,
            "diff at the requested paths touches 10 file(s), over the 5-file bound"
        ),
        other => panic!("expected Denied, got {other:?}"),
    }
}

#[tokio::test]
async fn an_exhausted_cap_escalates_even_under_rules_mode() {
    let dir = repo();
    std::fs::write(dir.path().join("src.rs"), "x").unwrap();
    let req = request(&["src.rs"], Some("test -f nonexistent-marker"));
    let (_precheck, decision) = evaluate(
        ScopeExpansionMode::Rules,
        &["src.rs".to_string()],
        Some(2),
        5,
        &GrantLedger::new(2), // already at the cap
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    assert_eq!(
        decision,
        Decision::Escalate,
        "cap exhaustion escalates rather than silently denying or granting"
    );
}

#[tokio::test]
async fn a_cap_not_yet_reached_does_not_escalate() {
    let dir = repo();
    std::fs::write(dir.path().join("src.rs"), "x").unwrap();
    let req = request(&["src.rs"], Some("test -f nonexistent-marker"));
    let (_precheck, decision) = evaluate(
        ScopeExpansionMode::Rules,
        &["src.rs".to_string()],
        Some(3),
        5,
        &GrantLedger::new(2),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    assert_eq!(decision, Decision::Granted);
}
