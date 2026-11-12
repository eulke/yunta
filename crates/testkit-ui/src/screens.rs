//! The states the terminal application's target frames show, each the same on every
//! machine: the application driven against runs a bench makes, with fixed ids, at the
//! instant every test stands at, and a person's keys pressed on it.
//!
//! - [`field`]: a repository's runs, one finished and one parked on its gate, which the
//!   attention row names;
//! - [`run`]: the run parked on its gate, its ribbon across the screen, its chronicle, and
//!   the page of its gate beside it;
//! - [`card`]: the run's decision, its card put to the person;
//! - [`desk`]: the plan it is on, read at its desk;
//! - [`confirmation`]: a run's stop asked over the field;
//! - [`palette`] and [`help`], each over the run.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use yunta_testkit::Bench;
use yunta_testkit_core::fixed_now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Performer, State};

use crate::driving::drive;
use crate::sandbox::workspace;

/// How long after its runs were made the application stands, in seconds: a run that has
/// waited a few minutes, as the screens' reference shows one.
const SECONDS_AFTER_THE_RUNS: i64 = 454;

/// One node that finishes.
const FINISHES: &str = "name: done\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";

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

/// A mock fixture with no sessions.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The config the benches' repository runs with: the planner on the mock adapter.
const CONFIG: &str = "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
                      - { adapter: mock, model: mock-model }\n";

/// The application open on a repository's runs: one finished, and one parked on its gate.
pub async fn field() -> State {
    let world = World::made().await;
    world.driven(&[]).await
}

/// The run parked on its gate, opened from the field, the page of its gate beside it: a
/// node that produced no file, so the page names no path of the bench's.
pub async fn run() -> State {
    World::made()
        .await
        .driven(&[Stroke::Enter, Stroke::Down, Stroke::Enter])
        .await
}

/// The run's decision, its card put to the person.
pub async fn card() -> State {
    World::made()
        .await
        .driven(&[Stroke::Enter, Stroke::Insert('a')])
        .await
}

/// The plan the run's decision is on, read at its desk.
pub async fn desk() -> State {
    World::made()
        .await
        .driven(&[Stroke::Enter, Stroke::Insert('a'), Stroke::Insert('r')])
        .await
}

/// Stopping the parked run, asked over the field.
pub async fn confirmation() -> State {
    World::made().await.driven(&[Stroke::Insert('c')]).await
}

/// The palette, over the run.
pub async fn palette() -> State {
    World::made()
        .await
        .driven(&[Stroke::Enter, Stroke::Insert('/')])
        .await
}

/// Help, over the run.
pub async fn help() -> State {
    World::made()
        .await
        .driven(&[Stroke::Enter, Stroke::Insert('?')])
        .await
}

/// A repository whose runs a bench made, and the application's work answered there.
struct World {
    parked: Bench,
    _finished: Bench,
}

impl World {
    /// The repository's two runs, each with its fixed id, made by its own bench beside the
    /// other.
    async fn made() -> World {
        let parked = Bench::with_run_id("01M7SCREEN0000000000Q78GRR");
        let config = parked.worktree.join(".yunta/config.yaml");
        tokio::fs::create_dir_all(config.parent().expect("the repository's .yunta"))
            .await
            .expect("its .yunta");
        tokio::fs::write(&config, CONFIG).await.expect("its config");
        parked.run(PLANS_THEN_ASKS, PLANNER).await;
        let finished = parked.beside("01M7SCREEN0000000000EKPBGE");
        finished.run(FINISHES, NO_SESSIONS).await;
        World {
            parked,
            _finished: finished,
        }
    }

    /// The application opened in the repository, its work answered, then `strokes`
    /// pressed, each once the state waits on nothing.
    async fn driven(&self, strokes: &[Stroke]) -> State {
        let root: &Path = self
            .parked
            .store_path()
            .parent()
            .expect("a bench's log stands in its root");
        let performer = Arc::new(Performer::new(workspace(&self.parked.worktree, root)));
        let (opened, asked) = State::opened(stands_at());
        drive(opened, asked, &performer, strokes).await.0
    }
}

/// The instant the screens stand at: some minutes after the bench made the runs.
fn stands_at() -> DateTime<Utc> {
    fixed_now() + Duration::seconds(SECONDS_AFTER_THE_RUNS)
}
