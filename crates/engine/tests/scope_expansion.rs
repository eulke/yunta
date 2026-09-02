//! `scope_expansion::evaluate` — the engine's own
//! decision over an agent's expansion request, exercised directly against
//! a real git worktree (the pre-check and the `rules`-mode size bound
//! both need real `git diff`).

use std::path::Path;

use yunta_core::ProposedCriterionEntry;
use yunta_core::ScopeExpansionMode;
use yunta_engine::process::Supervision;
use yunta_engine::scope_expansion::{evaluate, Decision, GrantLedger, ScopeExpansionRequest};

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    std::fs::write(dir.path().join(".gitkeep"), "").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "initial"]);
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
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    assert_eq!(precheck, Some(0));
    match decision {
        Decision::Denied(reason) => assert!(reason.contains("already passes")),
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
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    match decision {
        Decision::Denied(reason) => assert!(reason.contains("deny")),
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
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    match decision {
        Decision::Denied(reason) => assert!(reason.contains("within")),
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
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    match decision {
        Decision::Denied(reason) => assert!(reason.contains("proposed_criterion")),
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
        &GrantLedger::new(0),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    match decision {
        Decision::Denied(reason) => assert!(reason.contains("bound")),
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
        &GrantLedger::new(2),
        &req,
        dir.path(),
        Supervision::none(),
    )
    .await
    .unwrap();
    assert_eq!(decision, Decision::Granted);
}
