//! The small runs parked on what a person answers: a console's gate, a gate
//! that shows the release notes a node wrote, a departure, the token cap, the
//! loop cap, a failure, a plan sent back from its gate, a round whose answers
//! were refused, and a decision whose shown document cannot be read. Each parks
//! as a headless wake parks it, with no one at its terminal; the plan and the
//! round are then answered as another process answers them, and woken.

use yunta_core::events::{ArtifactAcceptedPayload, ArtifactEvent, ArtifactId, EventPayload};
use yunta_core::ArtifactKind;
use yunta_testkit::MOCK_CONFIG;

use super::stand_ins::AtTheTerminal;
use super::{appended, id, woken, woken_alone, SmallRun, NO_SESSIONS, PUBLISHES};
use crate::designed::place::Place;
use crate::designed::script::{self, Act, Step, OPENED};

/// A gate on a pull request with no forge reachable: no one at the terminal
/// answers it, so the run parks on the console's decision.
pub async fn console_gate() -> SmallRun {
    SmallRun::of(woken_alone("C0NS0E", PUBLISHES, NO_SESSIONS, MOCK_CONFIG).await)
}

/// A node that writes the release notes of a change, and a gate that shows
/// them before they are published, sending them back to be written again.
const NOTES_THEN_GATE: &str = r#"
name: release-notes
nodes:
  - id: notes
    kind: prompt
    runner: executor
    prompt: "Write the release notes of the change."
    artifacts:
      produces: [release-notes.md]
  - id: publish
    kind: gate
    depends_on: [notes]
    assignee: lead
    message: "The release notes, as they will be published. Publish them?"
    options: [publish, revise]
    on: { revise: notes }
    shows:
      - { node: notes, name: release-notes.md }
"#;

/// The notes' session: it writes the notes the gate shows, and the changelog's
/// entry the change carries.
const WRITES_NOTES: &str = r#"
dialect: claude-code
sessions:
  - effects:
      - path: "{{run.staging}}/notes/release-notes.md"
        content: |
          # Release notes

          - `yunta pack add --global` installs a pack once for every project of yours.
          - `yunta pack list --global` lists the packs installed that way.
      - path: CHANGELOG.md
        content: |
          ## Unreleased

          - Packs install for every project of yours with `--global`.
    steps:
      - { type: file_call, action: write, path: release-notes.md }
      - { type: file_call, action: write, path: CHANGELOG.md }
    outcome: { type: completed, summary: "Wrote the release notes and the changelog's entry." }
"#;

/// A gate that shows the release notes a node wrote, a document that is no
/// plan: no one at the terminal answers it, so the run parks on the gate's own
/// decision, whose menu the engine rebuilds from the pause.
pub async fn notes_gate() -> SmallRun {
    SmallRun::of(woken_alone("RN0TES", NOTES_THEN_GATE, WRITES_NOTES, MOCK_CONFIG).await)
}

/// A planner, then a loop whose task's session declares a departure from the
/// plan.
const DEPARTS: &str = r#"
name: departs
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

/// The plan the departure departs from: a shape the task owns, and the task.
const GREETING_PLAN: &str = "shapes:\n  - { name: Greeting, owner: greet, file: greeting.txt, \
                             code: \"Hello, and good night\" }\ntasks:\n  - id: greet\n    \
                             title: \"Write the greeting\"\n    scope: [\"greeting.txt\"]\n    \
                             criteria:\n      - cmd: \"test -f greeting.txt\"\n";

/// The task's session, which writes less than the plan's shape and says so.
const DEPARTING: &str = r#"  - match_prompt_contains: "`greet`"
    effects:
      - { path: greeting.txt, content: "Hello" }
    steps:
      - type: run_tool
        tool: yunta_declare_deviation
        arguments:
          kind: departure
          from: { shape: Greeting }
          planned: "Hello, and good night"
          instead: "Hello"
          why: "the second half needs the clock, which this task may not read"
    outcome: { type: completed, summary: wrote-hello }
"#;

/// A task's session departs from its plan, and no one at the terminal answers
/// it: the run parks owing the departure's decision.
pub async fn departure() -> SmallRun {
    let fixture = format!("{}{DEPARTING}", planned(GREETING_PLAN));
    SmallRun::of(woken_alone("DEPART", DEPARTS, &fixture, MOCK_CONFIG).await)
}

/// A node and the correction its failure reroutes to.
const SPENDS: &str = r#"
name: budget
nodes:
  - id: first
    kind: prompt
    runner: executor
    prompt: "Do the first thing."
    on_failure: { goto: fix, max_reroutes: 2 }
  - id: fix
    kind: prompt
    runner: executor
    prompt: "Fix it."
"#;

/// The executor on the mock, and a token cap its first session overshoots.
const CAPPED_TOKENS: &str = "runners:\n  executor:\n    - { adapter: mock, model: mock-model }\n\
                             limits:\n  max_tokens_per_run: 100\n";

/// A session that spends past the cap in one burst, then the ones after it.
const OVERSPENDS: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 150, output_tokens: 50 }
    outcome: { type: completed, summary: "spent a lot" }
  - outcome: { type: completed, summary: "fixed" }
  - outcome: { type: completed, summary: "did it" }
"#;

/// A run whose first session spends past its token cap: the run parks on the
/// cap's decision.
pub async fn token_cap() -> SmallRun {
    SmallRun::of(woken_alone("T0KENS", SPENDS, OVERSPENDS, CAPPED_TOKENS).await)
}

/// A planner, then a loop over its tasks.
const LOOPS: &str = r#"
name: loop-cap
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;

/// The planner and the executor on the mock, and a loop's cap of two
/// iterations.
const CAPPED_LOOPS: &str = "runners:\n  planner:\n    - { adapter: mock, model: mock-model }\n  \
                            executor:\n    - { adapter: mock, model: mock-model }\n\
                            limits:\n  max_loop_iterations: 2\n";

/// Three tasks, each after the one before, so the loop wants a third
/// iteration.
const CHAINED: &str = "tasks:\n  - id: T001\n    title: \"a\"\n    scope: [\"a.txt\"]\n    criteria:\n      \
                       - cmd: \"test -f a.txt\"\n  - id: T002\n    title: \"b\"\n    scope: [\"b.txt\"]\n    \
                       criteria:\n      - cmd: \"test -f b.txt\"\n    depends_on: [T001]\n  - id: T003\n    \
                       title: \"c\"\n    scope: [\"c.txt\"]\n    criteria:\n      - cmd: \"test -f c.txt\"\n    \
                       depends_on: [T002]\n";

/// Each task's session, which writes its file.
const BUILDS_EACH: &str = "  - effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: \"did T001\" }\n\
                           \x20 - effects:\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: \"did T002\" }\n\
                           \x20 - effects:\n      - { path: c.txt, content: \"c\" }\n    outcome: { type: completed, summary: \"did T003\" }\n";

/// A loop that wants a third iteration past a cap of two: the run parks on
/// the loop cap's decision.
pub async fn loop_cap() -> SmallRun {
    let fixture = format!("{}{BUILDS_EACH}", planned(CHAINED));
    SmallRun::of(woken_alone("CAPPED", LOOPS, &fixture, CAPPED_LOOPS).await)
}

/// A command that fails until a file it looks for stands.
const FAILS: &str = "name: plain-failure\nnodes:\n  - id: broken\n    kind: bash\n    \
                     run: \"test -f fixed.txt\"\n";

/// A command fails, and no one at the terminal answers its failure: the run
/// parks on the failure's decision, retry or abort.
pub async fn failure() -> SmallRun {
    SmallRun::of(woken_alone("FAVLTY", FAILS, NO_SESSIONS, MOCK_CONFIG).await)
}

/// A plan, then a gate that shows it and sends it back on `adjust`.
const PLAN_THEN_GATE: &str = yunta_testkit::workflows::PLAN_THEN_GATE;

/// The planner on the mock.
const PLANNER: &str = "runners:\n  planner:\n    - { adapter: mock, model: mock-model }\n";

/// The plan the planner hands over, both times.
const TWO_TASKS: &str = "summary: \"Make the two files\"\ndescription: \"Writes the files the run is about.\"\n\
                         tasks:\n  - { id: T001, title: \"Make it\", description: \"Writes made.txt.\", \
                         scope: [made.txt], changes: [{ at: made.txt, what: \"the file\", code: \"made\" }], \
                         outcome: \"made.txt exists\", criteria: [{ cmd: \"test -f made.txt\", proves: \"the file exists\" }] }\n  \
                         - { id: T002, title: \"Say it\", description: \"Writes said.txt.\", scope: [said.txt], \
                         changes: [{ at: said.txt, what: \"the words\", code: \"said\" }], outcome: \"said.txt exists\", \
                         criteria: [{ cmd: \"test -f said.txt\", proves: \"the words are there\" }] }\n";

/// Moving the clock and letting the planner's first session go, up to the
/// gate.
const PLANNING: &[Act] = &[
    Act::Logged(OPENED, Some("plan"), 1),
    Act::Moves(95),
    Act::Gives("plan-1"),
];

/// The planner's second session, sent back, up to the gate again.
const PLANNING_AGAIN: &[Act] = &[
    Act::Logged(OPENED, Some("plan"), 2),
    Act::Moves(140),
    Act::Gives("plan-2"),
];

/// A plan whose gate a person answers `adjust`, which sends the plan back: the
/// planner works again, the gate asks again, and the plan is approved on its
/// second attempt. The run finishes.
pub async fn sent_back() -> SmallRun {
    let fixture = format!(
        "{}{}",
        held_planner(TWO_TASKS, "plan-1"),
        held_session(TWO_TASKS, "plan-2")
    );
    let place = Place::of(&id("ADJVST"), &[], |bench| bench);
    place
        .bench
        .create(PLAN_THEN_GATE, &place.fixture(&fixture), PLANNER)
        .await;
    script::played(
        &place,
        &[Step::Moves(2), Step::Wakes(PLANNING), Step::Moves(180)],
    )
    .await;
    place
        .bench
        .answer_parked("adjust", Some("Split the second task in two."))
        .await
        .expect("the plan's gate takes `adjust`");
    let timeline = [
        Step::Wakes(PLANNING_AGAIN),
        Step::Moves(60),
        Step::Decides("approve"),
        Step::Wakes(&[]),
    ];
    script::played(&place, &timeline).await;
    SmallRun::of(place)
}

/// A node that hands a round of questions over.
const ASKS: &str = r#"
name: ask
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Ask what you need to know before continuing."
    artifacts:
      produces: [questions]
"#;

/// The round: a name typed, which nobody can do without, and a choice one can.
const ROUND: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - id: q1
                text: "What is the release called?"
                answer_type: text
                required: true
              - id: q2
                text: "Which environment?"
                answer_type: choice
                values: [staging, production]
                required: false
                assumes: "staging"
    outcome: { type: completed, summary: "asked" }
"#;

/// What the release is called, as a person types it.
const RELEASE: &str = "Spring release";

/// A round answered at the terminal leaving its required question out, which
/// the engine refuses, then answered whole as another process answers it, the
/// name typed. The run finishes.
pub async fn refused() -> SmallRun {
    let place = Place::of(&id("REFVSE"), &[], |bench| bench);
    woken(&place, ASKS, ROUND, MOCK_CONFIG).await;
    place.moves(75);
    let leaves_one_out = AtTheTerminal {
        answers: &[("q2", "production")],
        choice: None,
    };
    place.bench.wake_answering(&leaves_one_out).await;
    let timeline = [
        Step::Moves(40),
        Step::Round("ask", &[("q1", RELEASE), ("q2", "production")]),
        Step::Wakes(&[]),
    ];
    script::played(&place, &timeline).await;
    SmallRun::of(place)
}

/// The planner, the node that specifies its tasks, and the gate that shows both.
const SPECIFIED: &str = r#"
name: specified
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: spec
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [plan]
    prompt: "Write the tests the plan's tasks are held to."
    artifacts:
      produces: [spec]
  - id: approve-plan
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: "Plan and its tests registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows:
      - { node: plan, kind: tasks }
      - { node: spec, kind: spec }
"#;

/// A plan that writes a greeting, and the spec that holds it to its test.
const SPECIFYING: &str = r#"
dialect: claude-code
sessions:
  - match_prompt_contains: "Hand over the tasks"
    steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Greet"
            description: "Writes the greeting the run is about."
            tasks:
              - { id: greet, title: "Write the greeting", description: "Writes greeting.txt, and a check of it.", scope: [greeting.txt, tests/greet.sh], changes: [{ at: greeting.txt, what: "the greeting", code: "Hello" }, { at: tests/greet.sh, what: "a check of the greeting", code: "test -s greeting.txt" }], outcome: "greeting.txt says hello", criteria: [{ cmd: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello", proves: "the greeting says hello" }] }
    outcome: { type: completed, summary: planned }
  - match_prompt_contains: "Write the tests"
    steps:
      - type: run_tool
        tool: yunta_submit_spec
        arguments:
          document:
            specs:
              - { task: greet, files: [{ path: tests/greet.sh, content: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello\n" }], tests: [{ cmd: "sh tests/greet.sh", proves: "the greeting says hello" }] }
    outcome: { type: completed, summary: specified }
"#;

/// Bytes a spec no longer reads as: its specs are no list.
const SPEC_OF_ANOTHER_SHAPE: &[u8] = b"specs: 7\n";

/// A plan's gate parked, whose spec the run then holds as bytes that fail
/// today's shape, as a binary that read specs another way accepted them: what
/// the run waits on cannot be read.
pub async fn unread() -> SmallRun {
    let place = Place::of(&id("VNREAD"), &[], |bench| bench);
    let planner = "runners:\n  planner:\n    - { adapter: mock, model: mock-model }\n";
    woken(&place, SPECIFIED, SPECIFYING, planner).await;
    place.moves(30);
    accepted_again(&place, "spec", ArtifactKind::Spec, SPEC_OF_ANOTHER_SHAPE).await;
    SmallRun::of(place)
}

/// The document of `kind` that `node` handed over, accepted again as `bytes`,
/// under their own hash, at the run's clock.
async fn accepted_again(place: &Place, node: &str, kind: ArtifactKind, bytes: &[u8]) {
    let artifact = ArtifactId::Interpreted { kind };
    let accepted = place
        .bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Artifacts(ArtifactEvent::Accepted(accepted)))
                if accepted.artifact == artifact =>
            {
                Some(accepted.clone())
            }
            _ => None,
        })
        .expect("the run accepted the document");
    let hash = yunta_core::sha256_hex(bytes);
    let object = yunta_core::run_dir::object(&place.bench.run_dir(), &hash);
    tokio::fs::write(&object, bytes)
        .await
        .expect("the bytes, held as the run's object");
    let again = ArtifactAcceptedPayload {
        content_hash: hash,
        ..accepted
    };
    let payload = EventPayload::Artifacts(ArtifactEvent::Accepted(again));
    appended(place, Some(node), payload);
}

/// The mock's fixture whose one session, a planner's, hands `tasks` over: a
/// tasks document, as YAML.
fn planned(tasks: &str) -> String {
    format!(
        "dialect: claude-code\nsessions:\n{}",
        submits(tasks, "planned", None)
    )
}

/// The mock's fixture whose first session, a planner's, waits for `word`, then
/// hands `tasks` over.
fn held_planner(tasks: &str, word: &str) -> String {
    format!(
        "dialect: claude-code\nsessions:\n{}",
        submits(tasks, "planned", Some(word))
    )
}

/// A planner's session that waits for `word`, then hands `tasks` over again.
fn held_session(tasks: &str, word: &str) -> String {
    submits(tasks, "planned again", Some(word))
}

/// A planner's session that hands `tasks` over, once `word` is given when it
/// names one.
fn submits(tasks: &str, summary: &str, word: Option<&str>) -> String {
    let document: String = tasks
        .lines()
        .map(|line| format!("            {line}\n"))
        .collect();
    let held = word
        .map(|word| format!("      - {{ type: awaits_word, word: \"@words@/{word}\" }}\n"))
        .unwrap_or_default();
    format!(
        "  - steps:\n{held}      - type: run_tool\n        tool: yunta_submit_tasks\n        \
         arguments:\n          document:\n{document}    outcome: {{ type: completed, summary: \
         \"{summary}\" }}\n"
    )
}
