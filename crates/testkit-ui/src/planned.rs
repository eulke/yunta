//! A run parked on its plan's gate: a planner on the mock adapter hands two tasks over,
//! and a gate that shows them asks whether the plan goes on. The screens' world and the
//! walk's home each make one.

use yunta_testkit::Bench;

/// A plan, then a gate that shows it: the run parks on the gate, its plan beside it.
const PLANS_THEN_ASKS: &str = r#"
name: planned
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
"#;

/// The planner's session: two tasks handed over.
const PLANNER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Say hello"
            description: "Writes the greeting the program says."
            tasks:
              - { id: T001, title: "Write the greeting", description: "Writes hello.txt.", scope: [hello.txt], changes: [{ at: hello.txt, what: "the greeting", code: "hello" }], outcome: "hello.txt says hello", criteria: [{ cmd: "test -f hello.txt", proves: "the greeting exists" }] }
              - { id: T002, title: "Say it twice", description: "Writes twice.txt.", scope: [twice.txt], changes: [{ at: twice.txt, what: "the greeting twice", code: "hello hello" }], outcome: "twice.txt says it twice", criteria: [{ cmd: "test -f twice.txt", proves: "it is said twice" }] }
    outcome: { type: completed, summary: "planned" }
"#;

/// The config its repository runs with: the planner on the mock adapter.
const CONFIG: &str = "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
                      - { adapter: mock, model: mock-model }\n";

/// `bench`'s run, parked on its plan's gate: its repository set up with the planner,
/// then the run run.
pub(crate) async fn parked_on_its_plan(bench: &Bench) {
    let config = bench.worktree.join(".yunta/config.yaml");
    tokio::fs::create_dir_all(config.parent().expect("the repository's .yunta"))
        .await
        .expect("its .yunta");
    tokio::fs::write(&config, CONFIG).await.expect("its config");
    bench.run(PLANS_THEN_ASKS, PLANNER).await;
}
