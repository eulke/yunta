//! A scope request the engine refuses: the refusal is a finding that carries the
//! agent's own reason and the criterion it proposed.

use super::*;

#[tokio::test]
async fn an_already_passing_proposed_criterion_is_denied_without_consulting_even_in_ask_mode() {
    // A proposed criterion that already passes is rejected without consulting
    // anyone, in every mode, not even `ask`, which would otherwise escalate
    // and pause the run.
    let bench = Bench::new();

    let workflow = scope_expansion_workflow("ask", None);
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-p", "p", "a.txt", "test -f a.txt")
    );

    let mut fixture = plan_session(&tasks);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-p\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: did-p }}\n",
        request_step(serde_json::json!({
            "paths": ["c.txt"],
            "reason": "already fine, no work needed",
            "proposed_criterion": { "cmd": "true" },
        })),
    ));

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "an auto-rejected request must never pause the run, even under ask mode"
    );
    assert_eq!(
        state.tasks.status("task-p"),
        Some(yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_denied_for_task(
        &events,
        "task-p",
        yunta_core::expansion::Denial::PrecheckPasses,
    );
    let findings = findings_posted(&events);
    assert!(
        findings
            .iter()
            .any(|f| is_refusal_with_reason(f, "already fine, no work needed")),
        "the finding must carry the agent's own reason: {findings:?}"
    );
}

#[tokio::test]
async fn every_denial_becomes_a_finding_carrying_the_agent_s_reason_and_criterion() {
    // Every denial, here the default `deny` with no `scope_expansion:` block
    // in the workflow, becomes a finding that carries the agent's own reason
    // and proposed_criterion, not an explanation the engine made up.
    let bench = Bench::new();

    let workflow = no_scope_expansion_workflow();
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-d", "d", "a.txt", "test -f a.txt")
    );

    let mut fixture = plan_session(&tasks);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-d\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: did-d }}\n",
        request_step(serde_json::json!({
            "paths": ["b.txt"],
            "reason": "need an adjacent fix in b.txt",
            "proposed_criterion": { "cmd": "test -f b.txt" },
        })),
    ));

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("task-d"),
        Some(yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_denied_for_task(&events, "task-d", yunta_core::expansion::Denial::ModeDeny);
    let findings = findings_posted(&events);
    let finding = findings
        .iter()
        .find(|f| is_refusal_with_reason(f, "need an adjacent fix in b.txt"))
        .expect("a finding carrying the agent's own reason must exist");
    assert_finding_is_the_refusal(finding);
}

/// The log holds the denial of `task`'s request, and the denial's reason.
fn assert_denied_for_task(
    events: &[yunta_core::events::StoredEvent],
    task: &str,
    reason: yunta_core::expansion::Denial,
) {
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Scope(ScopeEvent::Denied(p)))
                if p.task_id.as_ref().is_some_and(|t| t.as_str() == task) =>
            {
                Some(p)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("a Denied event must be recorded for `{task}`"));
    assert_eq!(denied.why(), Some(reason));
}

/// Whether `finding` is a refused scope request that gives `reason`.
fn is_refusal_with_reason(finding: &yunta_core::events::Finding, reason: &str) -> bool {
    matches!(
        &finding.says,
        FindingSays::Found {
            cause: FindingCause::ScopeDenied {
                asked: Some(asked),
                ..
            },
        } if asked.reason.as_deref() == Some(reason)
    )
}

/// The finding a refused request leaves: the cause the engine found, the
/// criterion the agent proposed, and the location it asked to write.
fn assert_finding_is_the_refusal(finding: &yunta_core::events::Finding) {
    let FindingSays::Found { cause } = &finding.says else {
        panic!("a refused request is a finding the engine found: {finding:?}");
    };
    assert_eq!(
        cause,
        &FindingCause::ScopeDenied {
            asker: ScopeAsker::Task(yunta_core::TaskId::from("task-d")),
            denial: yunta_core::expansion::Denial::ModeDeny,
            asked: Some(AskedScope {
                paths: vec![yunta_core::ScopeGlob::from("b.txt")],
                reason: Some("need an adjacent fix in b.txt".to_string()),
            }),
        }
    );
    assert_eq!(
        finding.proposed_criterion,
        Some(yunta_core::events::ProposedCriterion {
            cmd: "test -f b.txt".to_string()
        })
    );
    assert_eq!(
        finding.location,
        "b.txt".into(),
        "the finding locates where the agent asked to write"
    );
}
