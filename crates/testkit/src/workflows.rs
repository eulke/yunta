//! Workflows that tests in more than one file run, each with what it is for.

/// A check that fails in either mode, its reroute spent at once, so the decision it parks
/// on offers `promote`: `quick` promotes to `full`, which has nowhere later to go.
pub const PROMOTABLE: &str = r#"
name: promotable
modes:
  quick: { include: [check, fix] }
  full:  { include: all }
nodes:
  - id: check
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix, max_reroutes: 0 }
  - id: fix
    kind: bash
    run: "touch fixed.txt"
"#;

/// One node that finishes, on no adapter: a run of it ends `finished` at once.
pub const FINISHES: &str = "name: done\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";

/// A gate, then a node once it is passed: a run parks on the gate, `Go on?`, whose one
/// option, `go`, lets it finish.
pub const GATED: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, \
                     message: \"Go on?\", options: [go] }\n  \
                     - { id: after, kind: bash, run: \"true\", depends_on: [approve] }\n";

/// A node that asks: the session of its runner, `executor`, hands a round of questions
/// over and ends, and the run parks on the round.
pub const ASKS: &str = "name: asking\nnodes:\n  - id: ask\n    kind: prompt\n    runner: executor\n    \
                    prompt: \"Ask what has to be known.\"\n    artifacts:\n      produces: [questions]\n";

/// The mock session that hands one question over, `What changed?`, which is answered
/// with text.
pub const HANDS_ONE_QUESTION_OVER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - { id: summary, text: "What changed?", answer_type: text, required: true }
    outcome: { type: completed, summary: asked }
"#;

/// The config a repository runs `executor` on the mock adapter with, every node in place.
pub const EXECUTOR_ON_THE_MOCK: &str = "defaults:\n  isolation: none\nrunners:\n  executor:\n    \
                      - { adapter: mock, model: mock-model }\n";

/// A plan, then a gate that shows it: a run parks on `Plan registered. Approve?`, its plan
/// beside it.
pub const PLAN_THEN_GATE: &str = r#"
name: reviewed
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

/// The planner's mock sessions: two tasks handed over, with a decision taken and a risk;
/// then, planned again, a note.
pub const PLANNER_HANDS_TWO_TASKS: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make the two files"
            description: "Writes the files the run is about."
            decisions:
              - { id: names, question: "What are they called?", choice: "made and said", alternatives: [a and b], why: "They say what they are." }
            risks: ["Two files can drift apart."]
            tasks:
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], changes: [{ at: made.txt, what: "the file", code: "the file's first line" }], outcome: "made.txt exists", criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }] }
              - { id: T002, title: "Say it", description: "Writes said.txt.", scope: [said.txt], depends_on: [T001], changes: [{ at: said.txt, what: "the words", code: "the words themselves" }], outcome: "said.txt exists", criteria: [{ cmd: "test -f said.txt", proves: "the words are there" }] }
    outcome: { type: completed, summary: "planned" }
  - steps:
      - { type: note, text: "planned again" }
    outcome: { type: completed, summary: "planned again" }
"#;

/// The config a repository runs `planner` on the mock adapter with, every node in place.
pub const PLANNER_ON_THE_MOCK: &str = "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
                                       - { adapter: mock, model: mock-model }\n";

/// A workflow that takes what was asked for as its request, `ask`, and finishes.
pub const GREETS: &str = "name: greet\ninputs:\n  ask: { type: string, request: true }\nnodes:\n  \
                      - { id: say, kind: bash, run: \"true\" }\n";
