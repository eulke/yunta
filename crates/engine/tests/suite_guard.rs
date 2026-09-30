//! Every task is held to the suite its run's lineage measured green: a
//! change that breaks what passed is judged against the task that made
//! it, while its session is still there to answer for it.

use yunta_core::events::{
    BaselineCapturedPayload, BaselineOrigin, BaselineResults, CriterionResult, CriterionType,
    EventPayload, NodeEvent, Phase, SessionEvent, TaskStatus,
};
use yunta_core::{Criterion, RunId, Task};
use yunta_engine::{judged_task, RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

const SUITE: &str = "test ! -f broken.txt";

fn measured(exit_code: i32, origin: BaselineOrigin) -> BaselineCapturedPayload {
    BaselineCapturedPayload {
        command: SUITE.to_string(),
        results: BaselineResults {
            exit_code,
            summary: String::new(),
        },
        hash: yunta_core::sha256_hex(b""),
        origin,
    }
}

fn task_with(criteria: Vec<Criterion>) -> Task {
    Task {
        id: "T001".into(),
        title: "Make it".to_string(),
        scope: vec!["made.txt".into()],
        criteria,
        depends_on: Vec::new(),
        notes: None,
        description: None,
    }
}

fn criterion(cmd: &str, r#type: Option<CriterionType>) -> Criterion {
    Criterion {
        cmd: cmd.to_string(),
        r#type,
        proves: None,
    }
}

/// The suite each criterion list is judged by, as `(cmd, is_guard)`.
fn commands(task: &Task) -> Vec<(&str, bool)> {
    task.criteria
        .iter()
        .map(|criterion| (criterion.cmd.as_str(), criterion.is_guard()))
        .collect()
}

#[test]
fn a_task_is_judged_by_the_suite_its_lineage_measured_green() {
    let task = task_with(vec![criterion("test -f made.txt", None)]);
    let judged = judged_task(&task, Some(&measured(0, BaselineOrigin::Measured)));
    assert_eq!(
        commands(&judged),
        vec![("test -f made.txt", false), (SUITE, true)]
    );
    assert!(
        judged.criteria[1].proves.is_some(),
        "the guard says what it is there to show: {judged:#?}"
    );
    assert_eq!(judged.id, task.id, "only the criteria change");
}

#[test]
fn a_red_measurement_adds_no_guard_to_any_task() {
    let task = task_with(vec![criterion("test -f made.txt", None)]);
    assert_eq!(
        judged_task(&task, Some(&measured(1, BaselineOrigin::Measured))),
        task
    );
    assert_eq!(judged_task(&task, None), task, "nor does no measurement");
}

#[test]
fn a_task_that_declares_the_suite_is_judged_by_its_own_declaration() {
    for declared in [Some(CriterionType::Guard), None] {
        let task = task_with(vec![
            criterion("test -f made.txt", None),
            criterion(&format!("  {SUITE} "), declared),
        ]);
        assert_eq!(
            judged_task(&task, Some(&measured(0, BaselineOrigin::Measured))),
            task
        );
    }
}

#[test]
fn an_inherited_measurement_holds_tasks_to_the_same_suite() {
    let task = task_with(vec![criterion("test -f made.txt", None)]);
    let inherited = measured(
        0,
        BaselineOrigin::Inherited {
            run: RunId::from("01JROOT0000000000000000000"),
        },
    );
    assert_eq!(
        commands(&judged_task(&task, Some(&inherited))),
        vec![("test -f made.txt", false), (SUITE, true)]
    );
}

// --- a run's loop -----------------------------------------------------------

fn config() -> String {
    format!("{MOCK_CONFIG}baseline:\n  suite: \"{SUITE}\"\n")
}

/// A plan that writes a one-task document whose scope is `scope`, and the
/// loop that works it. `before` runs ahead of both.
fn workflow(scope: &str, before: &str) -> String {
    format!(
        r#"
name: held-to-the-suite
nodes:{before}
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Make it\n    scope: {scope}\n    criteria:\n      - cmd: test -f made.txt\n' > {{{{node.artifacts}}}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#
    )
}

/// A session that does the task's work, and — when `breaks` — also
/// breaks what the suite checks.
fn session(breaks: bool) -> String {
    let broken = if breaks {
        "\n      - { path: broken.txt, content: broken }"
    } else {
        ""
    };
    format!(
        "\
capabilities: {{ run_tools: true }}
sessions:
  - effects:
      - {{ path: made.txt, content: made }}{broken}
    outcome: {{ type: completed, summary: made }}
"
    )
}

/// Every result the log holds for the suite in T001's checks of `phase`.
fn suite_checks(bench: &Bench, phase: Phase) -> Vec<CriterionResult> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p))) if p.phase == phase => {
                Some(p.results.clone())
            }
            _ => None,
        })
        .flatten()
        .filter(|result| result.cmd == SUITE)
        .collect()
}

fn status_of(bench: &Bench) -> Option<TaskStatus> {
    yunta_engine::derive(&bench.events()).tasks.status("T001")
}

#[tokio::test]
async fn every_task_is_checked_against_the_lineage_suite_before_and_after_its_work() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(&workflow("[made.txt]", ""), &session(false), &config())
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    for phase in [Phase::Pre, Phase::Post] {
        let checks = suite_checks(&bench, phase);
        assert!(!checks.is_empty(), "no {phase:?} check ran the suite");
        assert!(
            checks
                .iter()
                .all(|check| check.r#type == Some(CriterionType::Guard) && check.exit_code == 0),
            "{checks:#?}"
        );
    }
    assert_eq!(status_of(&bench), Some(TaskStatus::Done));
}

/// The incident this exists for: a task whose own criterion passes, but
/// whose change breaks what passed before the run, stays open — and says
/// the suite is what it left red.
#[tokio::test]
async fn a_task_whose_work_breaks_the_suite_does_not_close() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(
            &workflow("[made.txt, broken.txt]", ""),
            &session(true),
            &config(),
        )
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(status_of(&bench), Some(TaskStatus::Blocked));
    let last_post = suite_checks(&bench, Phase::Post);
    assert_eq!(
        last_post.last().map(|check| check.exit_code),
        Some(1),
        "the suite is what the attempt left red: {last_post:#?}"
    );
}

/// What broke the suite came before the task, so no session opens to be
/// blamed for it, and what blocks the task says what the suite is for.
#[tokio::test]
async fn a_task_whose_base_already_fails_the_suite_is_blocked_before_any_session() {
    let before = "
  - id: break
    kind: bash
    scope: [broken.txt]
    run: \"touch broken.txt\"";
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(&workflow("[made.txt]", before), &session(false), &config())
        .await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("the loop stops on the red base: {terminal:?}");
    };
    assert!(
        reason.contains(&format!(
            "guard `{SUITE}` is already red before any work started"
        )) && reason.contains("it is there to show that"),
        "{reason}"
    );
    assert!(
        !bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Session(SessionEvent::Opened(_)))
        )),
        "no session opened"
    );
}

/// The same, when what broke the suite is a node with no scope of its
/// own: its work is on the run's branch once it closes, so the task's
/// checkout — opened from that branch — sees it too.
#[tokio::test]
async fn a_task_whose_base_an_unscoped_node_broke_is_blocked_before_any_session() {
    let before = "
  - id: break
    kind: bash
    run: \"touch broken.txt\"";
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(&workflow("[made.txt]", before), &session(false), &config())
        .await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("the loop stops on the red base: {terminal:?}");
    };
    assert!(
        reason.contains(&format!(
            "guard `{SUITE}` is already red before any work started"
        )),
        "{reason}"
    );
    assert!(
        !bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Session(SessionEvent::Opened(_)))
        )),
        "no session opened"
    );
}

/// A comparison after the loop asks the suite about the tree the last
/// guard already checked: it reuses that answer, and says the suite
/// already ran there instead of naming a comparison that never ran.
#[tokio::test]
async fn a_compare_after_the_loop_reuses_what_the_guard_answered_on_that_tree() {
    let bench = Bench::new();
    let workflow = format!(
        "{}  - {{ id: tests, kind: check, builtin: baseline_compare, depends_on: [implement] }}\n",
        workflow("[made.txt]", "")
    );
    let RunReport { terminal, .. } = bench
        .run_with_config(&workflow, &session(false), &config())
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let outcome = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Finished(p)))
                if event
                    .node_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == "tests") =>
            {
                Some(p.outcome.clone())
            }
            _ => None,
        });
    assert_eq!(
        outcome.as_deref(),
        Some("no regression vs baseline (exit 0, reused: the suite already ran on this same tree)")
    );
}
