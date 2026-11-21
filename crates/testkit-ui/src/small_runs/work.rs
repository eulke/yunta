//! The small runs whose work the design's run does not show: a loop with two
//! tasks at work at once, and a run whose mode leaves a node out.
//!
//! The two tasks start together and would race for the pool's checkouts and
//! to the log, so each task's survey of the pool is held, and the second's
//! let go once the first's session opened in the checkout it took; then each
//! session waits for its words, given one at a time once the call the last
//! one let go is on the log, as the design's run is recorded.

use yunta_testkit::MOCK_CONFIG;

use super::{id, woken, SmallRun, NO_SESSIONS};
use crate::designed::place::Place;
use crate::designed::script::{self, Act, Step, OPENED, SAID};

/// A planner, then a loop that works two tasks at once.
const TWO_AT_ONCE: &str = r#"
name: independent-tasks
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
    concurrency: 2
    prompt: "Read your task from the tasks document and implement it."
"#;

/// The planner hands two tasks over; each task's session reads, waits, writes
/// its file and waits again, so both work at once.
const TWO_SESSIONS: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks:
              - id: task-1
                title: "Write the greeting"
                scope: ["greeting.txt"]
                criteria:
                  - cmd: "test -f greeting.txt"
              - id: task-2
                title: "Write the farewell"
                scope: ["farewell.txt"]
                criteria:
                  - cmd: "test -f farewell.txt"
      - { type: awaits_word, word: "@words@/plan-1" }
    outcome: { type: completed, summary: planned }
  - match_prompt_contains: "task-1"
    effects:
      - { path: greeting.txt, content: "hello" }
    steps:
      - { type: file_call, action: read, path: README.md }
      - { type: awaits_word, word: "@words@/task-1-1" }
      - { type: file_call, action: write, path: greeting.txt }
      - { type: awaits_word, word: "@words@/task-1-2" }
    outcome: { type: completed, summary: "wrote the greeting" }
  - match_prompt_contains: "task-2"
    effects:
      - { path: farewell.txt, content: "goodbye" }
    steps:
      - { type: file_call, action: read, path: README.md }
      - { type: awaits_word, word: "@words@/task-2-1" }
      - { type: file_call, action: write, path: farewell.txt }
      - { type: awaits_word, word: "@words@/task-2-2" }
    outcome: { type: completed, summary: "wrote the farewell" }
"#;

/// The planner let go, its plan checked, once the pool's surveys are held;
/// the first task's survey let go, then the second's once the first's
/// session made its first call; then both sessions' calls let go one at a
/// time, the clock moving between, the second's last word once the first
/// task's close is judged.
const BOTH_AT_WORK: &[Act] = &[
    Act::Logged(SAID, Some("plan"), 1),
    Act::Moves(50),
    Act::Arms("surveys"),
    Act::Gives("plan-1"),
    Act::LetsSurvey(2),
    Act::Logged(OPENED, Some("implement"), 1),
    Act::Logged(SAID, Some("implement"), 1),
    Act::LetsSurvey(1),
    Act::Logged(OPENED, Some("implement"), 2),
    Act::Logged(SAID, Some("implement"), 2),
    Act::Moves(70),
    Act::Gives("task-1-1"),
    Act::Logged(SAID, Some("implement"), 3),
    Act::Moves(25),
    Act::Gives("task-2-1"),
    Act::Logged(SAID, Some("implement"), 4),
    Act::Moves(40),
    Act::Gives("task-1-2"),
    Act::Logged("scope_checked", Some("implement"), 1),
    Act::Moves(30),
    Act::Gives("task-2-2"),
];

/// A loop working two tasks at once, each call naming its task's session: the
/// run finishes.
pub async fn two_tasks() -> SmallRun {
    let place = Place::of(&id("TW0TSK"), &[("README.md", "# Greetings\n")], |bench| {
        bench
    });
    place
        .bench
        .create(TWO_AT_ONCE, &place.fixture(TWO_SESSIONS), MOCK_CONFIG)
        .await;
    script::played(&place, &[Step::Moves(2), Step::Wakes(BOTH_AT_WORK)]).await;
    SmallRun::of(place)
}

/// Three commands; the mode `alpha` leaves the middle one out.
const MODED: &str = r#"
name: mode-scenario
modes:
  alpha:  { include: [start, ship] }
  gamma:  { include: all }
nodes:
  - id: start
    kind: bash
    run: "true"
  - id: extra
    kind: bash
    depends_on: [start]
    run: "true"
  - id: ship
    kind: bash
    depends_on: [extra]
    run: "true"
"#;

/// A run in a mode that leaves `extra` out: it finishes without it.
pub async fn mode_skips() -> SmallRun {
    let place = Place::of(&id("SK1PPD"), &[], |bench| bench.in_mode("alpha"));
    woken(&place, MODED, NO_SESSIONS, MOCK_CONFIG).await;
    SmallRun::of(place)
}
