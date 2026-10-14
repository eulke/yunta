//! What a run freezes at its birth, and the sessions the engine cannot stop: a
//! session whose kill is refused ends its node with that fact and keeps what it
//! spent, a loop task of the same kind ends its loop node and keeps its
//! checkout taken while its process group runs, and a node's confinement
//! holds across its wakes.

use super::*;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    AgentMessageType, EventPayload, NodeFailedPayload, StoredEvent, TokenUsage,
};

/// An agent's definition is read once, at birth: an edit a node makes to
/// the definition's file during the run does not reach the session after it.
#[tokio::test]
async fn an_agent_edited_mid_run_keeps_the_definition_the_run_was_born_with() {
    let bench = Bench::new();
    install_agent(&bench.worktree, "reviewer");
    let workflow = "name: frozen-agent\nnodes:\n  - { id: edit, kind: bash, run: \"echo 'Work as v2.' >> .yunta/agents/reviewer.md\" }\n  - { id: audit, kind: prompt, runner: reviewer, agent: reviewer, prompt: \"Audit.\", depends_on: [edit] }\n";
    let config = "runners:\n  reviewer:\n    - { adapter: mock, model: m }\n";
    let fixture =
        "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: audited }\n";

    let RunReport { terminal, .. } = bench.run_with_config(workflow, fixture, config).await;

    assert_eq!(terminal, RunTerminal::Finished);
    let agent = bench.mock().requests_seen()[0].agent.clone().unwrap();
    assert_eq!(agent.instructions.trim(), "Work as reviewer.");
}

// --- a session the engine cannot stop --------------------------------------

/// The `node_failed` payloads a node recorded, read back from the log.
fn failed_payloads(events: &[StoredEvent], node: &str) -> Vec<NodeFailedPayload> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(payload))) => Some(payload.clone()),
            _ => None,
        })
        .collect()
}

/// The failures a node recorded, read back from the log.
fn failures(events: &[StoredEvent], node: &str) -> Vec<Failure> {
    failed_payloads(events, node)
        .into_iter()
        .map(|payload| payload.failure)
        .collect()
}

/// Fires `token` once `count` events of `node` that `seen` accepts are on the
/// log, so the run is cut at a point the test names rather than after a delay.
async fn cancel_once_logged(
    bench: &Bench,
    token: &CancellationToken,
    node: &str,
    seen: fn(&EventPayload) -> bool,
    count: usize,
) {
    yunta_testkit::wait_until_async(
        || async move {
            bench
                .events()
                .iter()
                .filter(|event| {
                    event.node_id.as_ref().is_some_and(|id| id.as_str() == node)
                        && event.payload().is_some_and(seen)
                })
                .count()
                >= count
        },
        || format!("fewer than {count} events of `{node}` reached the log"),
    )
    .await;
    token.cancel();
}

/// Fires `token` once `count` of the node's sessions are on the log, so each
/// wake has its sessions of its own before it is cut.
async fn cancel_once_opened(bench: &Bench, token: &CancellationToken, node: &str, count: usize) {
    cancel_once_logged(
        bench,
        token,
        node,
        |payload| matches!(payload, EventPayload::Session(SessionEvent::Opened(_))),
        count,
    )
    .await;
}

/// Whether a payload is a session reporting what it spent.
fn reports_usage(payload: &EventPayload) -> bool {
    matches!(
        payload,
        EventPayload::Session(SessionEvent::Message(message))
            if message.message_type == AgentMessageType::Usage
    )
}

/// A session the engine must stop, whose kill is refused, ends its node with
/// the fact that the session could not be stopped, not as a cancellation.
#[tokio::test]
async fn a_session_whose_kill_fails_ends_its_node_with_session_not_stopped() {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());
    let workflow =
        "name: stuck\nnodes:\n  - { id: hang, kind: prompt, runner: executor, prompt: \"x\" }\n";
    let fixture = "dialect: claude-code\nsessions:\n  - kill: refuse\n    outcome: { type: hang, on_interrupt: ignore }\n";
    let config = "runners:\n  executor:\n    - { adapter: mock, model: m }\n";
    let (_run, ()) = tokio::join!(
        bench.run_with_config(workflow, fixture, config),
        cancel_once_opened(&bench, &token, "hang", 1),
    );

    let failed = failures(&bench.events(), "hang");
    assert!(
        failed
            .iter()
            .any(|failure| matches!(failure, Failure::SessionNotStopped { .. })),
        "{failed:?}"
    );
}

/// A session the engine stops at its token cap, whose kill is refused, ends
/// its node with the fact that the session could not be stopped.
#[tokio::test]
async fn a_session_stopped_at_its_token_cap_whose_kill_fails_ends_its_node_with_session_not_stopped(
) {
    let bench = Bench::new();
    let workflow =
        "name: stuck\nnodes:\n  - { id: hang, kind: prompt, runner: executor, prompt: \"x\" }\n";
    // The session reports 200 tokens against a cap of 100, so the engine stops
    // it itself, while the run is live.
    let fixture = "dialect: claude-code\nsessions:\n  - kill: refuse\n    steps:\n      - { type: usage, input_tokens: 200, output_tokens: 0 }\n    outcome: { type: hang, on_interrupt: ignore }\n";
    let config = "runners:\n  executor:\n    - { adapter: mock, model: m }\nlimits:\n  max_tokens_per_run: 100\n";
    // The terminal is not what the test reads: the node's failure is on the log.
    bench.run_with_config(workflow, fixture, config).await;

    let failed = failures(&bench.events(), "hang");
    assert!(
        failed
            .iter()
            .any(|failure| matches!(failure, Failure::SessionNotStopped { .. })),
        "{failed:?}"
    );
}

/// A session that reported what it spent, then could not be stopped, leaves
/// that spend on its node's failure, so the run's spend still counts it.
#[tokio::test]
async fn a_session_that_cannot_be_stopped_leaves_its_spend_on_the_node() {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());
    let workflow =
        "name: spent\nnodes:\n  - { id: hang, kind: prompt, runner: executor, prompt: \"x\" }\n";
    let fixture = "dialect: claude-code\nsessions:\n  - kill: refuse\n    steps:\n      - { type: usage, input_tokens: 30, output_tokens: 12 }\n    outcome: { type: hang, on_interrupt: ignore }\n";
    let config = "runners:\n  executor:\n    - { adapter: mock, model: m }\n";
    let (_run, ()) = tokio::join!(
        bench.run_with_config(workflow, fixture, config),
        cancel_once_logged(&bench, &token, "hang", reports_usage, 1),
    );

    let spent: Vec<TokenUsage> = failed_payloads(&bench.events(), "hang")
        .into_iter()
        .map(|payload| payload.tokens_used)
        .collect();
    assert_eq!(
        spent,
        [TokenUsage {
            input: 30,
            output: 12,
            cached: None,
        }]
    );
}

/// A loop task whose session cannot be stopped fails the loop node with that
/// fact.
#[tokio::test]
async fn a_loop_task_whose_session_cannot_be_stopped_fails_the_loop_node() {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());
    let workflow = concurrency_workflow(1);
    let fixture = format!(
        "{}  - match_prompt_contains: \"task-1\"\n    kill: refuse\n    outcome: {{ type: hang, on_interrupt: ignore }}\n",
        plan_session(&independent_tasks(1)),
    );
    let (_run, ()) = tokio::join!(
        bench.run_with_config(&workflow, &fixture, yunta_testkit::MOCK_CONFIG),
        cancel_once_opened(&bench, &token, "implement", 1),
    );

    let failed = failures(&bench.events(), "implement");
    assert!(
        failed
            .iter()
            .any(|failure| matches!(failure, Failure::SessionNotStopped { .. })),
        "{failed:?}"
    );
}

/// A loop batch whose first task completes and whose second task reports more
/// spend than its own cap, and whose kill is refused, integrates the first
/// task before the node fails with that fact: the first task is done in the
/// log, and the second one is not.
#[tokio::test]
async fn a_loop_batch_integrates_the_tasks_before_an_unstoppable_one() {
    let bench = Bench::new();
    let workflow = concurrency_workflow(2);
    let fixture = format!(
        "{}  - match_prompt_contains: \"task-1\"\n    effects:\n      - {{ path: out-1.txt, content: \"1\" }}\n    outcome: {{ type: completed, summary: \"did task-1\" }}\n  - match_prompt_contains: \"task-2\"\n    kill: refuse\n    steps:\n      - {{ type: usage, input_tokens: 200, output_tokens: 0 }}\n    outcome: {{ type: hang, on_interrupt: ignore }}\n",
        plan_session(&independent_tasks(2)),
    );
    // The cap is the session budget each session is handed: task two's own
    // spend passes it, and the engine stops that session; task one spends nothing.
    let config = format!(
        "{}limits:\n  max_tokens_per_run: 100\n",
        yunta_testkit::MOCK_CONFIG
    );
    let RunReport { state, .. } = bench.run_with_config(&workflow, &fixture, &config).await;

    let failed = failures(&bench.events(), "implement");
    assert!(
        failed
            .iter()
            .any(|failure| matches!(failure, Failure::SessionNotStopped { .. })),
        "{failed:?}"
    );
    assert_eq!(
        state.tasks.status("task-1"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert_ne!(
        state.tasks.status("task-2"),
        Some(yunta_core::events::TaskStatus::Done)
    );
}

/// A loop batch whose root cancellation cuts both of its sessions: the first
/// ends on the interrupt, and the second cannot be stopped. The node fails with
/// the session it could not stop, not with the cancellation that came first.
#[tokio::test]
async fn a_cancelled_batch_with_a_session_that_cannot_be_stopped_fails_the_loop_node() {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());
    let workflow = concurrency_workflow(2);
    let fixture = format!(
        "{}  - match_prompt_contains: \"task-1\"\n    outcome: {{ type: hang, on_interrupt: end }}\n  - match_prompt_contains: \"task-2\"\n    kill: refuse\n    outcome: {{ type: hang, on_interrupt: ignore }}\n",
        plan_session(&independent_tasks(2)),
    );
    let (_run, ()) = tokio::join!(
        bench.run_with_config(&workflow, &fixture, yunta_testkit::MOCK_CONFIG),
        cancel_once_opened(&bench, &token, "implement", 2),
    );

    let failed = failures(&bench.events(), "implement");
    assert!(
        failed
            .iter()
            .any(|failure| matches!(failure, Failure::SessionNotStopped { .. })),
        "{failed:?}"
    );
}

/// A loop task whose session the engine could not stop keeps its checkout
/// taken while the session's process group runs, since that group may still
/// write in it.
#[tokio::test]
async fn a_loop_task_whose_session_cannot_be_stopped_keeps_its_checkout_taken() {
    let bench = Bench::new();
    let workflow = concurrency_workflow(1);
    let fixture = format!(
        "{}  - match_prompt_contains: \"task-1\"\n    kill: refuse\n    steps:\n      - {{ type: usage, input_tokens: 200, output_tokens: 0 }}\n    outcome: {{ type: hang, on_interrupt: ignore }}\n",
        plan_session(&independent_tasks(1)),
    );
    let config = format!(
        "{}limits:\n  max_tokens_per_run: 100\n",
        yunta_testkit::MOCK_CONFIG
    );
    let _run = bench.run_with_config(&workflow, &fixture, &config).await;

    let held = slot_locks(&bench.pool());
    let groups = registered_groups(&bench.run_dir());
    // Killed before any assertion, so the test leaves no process behind.
    for group in &groups {
        yunta_core::process::group::force_kill_group(*group)
            .await
            .expect("the session's group is killed");
    }
    assert!(
        !groups.is_empty(),
        "the session's group stays registered while it runs"
    );
    assert!(
        !held.is_empty(),
        "the task's checkout stays taken while its session's group runs"
    );
}

/// The checkout locks of the pool at `pool`.
fn slot_locks(pool: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(pool)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "lock")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("slot-"))
        })
        .collect()
}

/// The process groups the run's registry names.
fn registered_groups(run_dir: &std::path::Path) -> Vec<yunta_core::Pid> {
    match yunta_engine::read_registry(run_dir) {
        yunta_engine::Registry::Read(registry) => registry.doc.process_groups.clone(),
        _ => Vec::new(),
    }
}

// --- a node's confinement across two wakes ---------------------------------

const WORK_WORKFLOW: &str =
    "name: twice\nnodes:\n  - { id: work, kind: prompt, runner: executor, prompt: work }\n";
/// A single session that opens and then never ends on its own.
const HANG_ONCE: &str = "dialect: claude-code\nsessions:\n  - { outcome: { type: hang } }\n";
const COMPLETE_ONCE: &str =
    "dialect: claude-code\nsessions:\n  - { outcome: { type: completed, summary: done } }\n";

#[tokio::test]
async fn the_same_node_gets_the_same_confinement_on_two_wakes() {
    let token = tokio_util::sync::CancellationToken::new();
    let bench = yunta_testkit::Bench::new().with_cancel(token.clone());
    let (first, ()) = tokio::join!(
        bench.run(WORK_WORKFLOW, HANG_ONCE),
        cancel_once_opened(&bench, &token, "work", 1),
    );
    let yunta_engine::RunReport { terminal, .. } = first;
    assert!(
        matches!(terminal, yunta_engine::RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    // The checkout's config changes between the two wakes; the second wake
    // must not read it.
    yunta_testkit::write(
        &bench.worktree.join(".yunta/config.yaml"),
        "permissions:\n  network:\n    default: false\n",
    );
    let bench = bench.with_cancel(tokio_util::sync::CancellationToken::new());
    let yunta_engine::RunReport { terminal, .. } = bench.wake_on_fixture(COMPLETE_ONCE).await;
    assert_eq!(terminal, yunta_engine::RunTerminal::Finished);

    let confinements: Vec<_> = bench
        .events()
        .into_iter()
        .filter(|event| {
            event
                .node_id
                .as_ref()
                .is_some_and(|node| node.as_str() == "work")
        })
        .filter_map(|event| match event.payload() {
            Some(yunta_core::events::EventPayload::Session(
                yunta_core::events::SessionEvent::Opened(opened),
            )) => Some(opened.confinement.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        confinements.len(),
        2,
        "one session per wake: {confinements:?}"
    );
    assert_eq!(confinements[0], confinements[1]);
}

/// A frozen input the run no longer holds is refused at the node that reads
/// it: the node fails with that input named, and no session opens for it.
#[tokio::test]
async fn an_agent_the_run_no_longer_holds_fails_the_node_that_reads_it() {
    let bench = Bench::new();
    install_agent(&bench.worktree, "reviewer");
    let workflow = "name: frozen-gone\nnodes:\n  - { id: remove, kind: bash, run: \"rm -rf {{run.dir}}/objects\" }\n  - { id: audit, kind: prompt, runner: reviewer, agent: reviewer, prompt: \"Audit.\", depends_on: [remove] }\n";
    let config = "runners:\n  reviewer:\n    - { adapter: mock, model: m }\n";
    let fixture =
        "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: audited }\n";

    let RunReport { .. } = bench.run_with_config(workflow, fixture, config).await;

    let failed = failures(&bench.events(), "audit");
    assert!(
        failed.iter().any(|failure| matches!(
            failure,
            Failure::FrozenInput { frozen_input } if frozen_input == "agents/reviewer.md"
        )),
        "{failed:?}"
    );
    assert!(
        bench.mock().requests_seen().is_empty(),
        "the node opened no session"
    );
}
