//! The two runs every run's screen stands beside in its field: the first of each kind the
//! design's own field lists, a run of fragua in `quick` whose `grill` failed, needing the
//! person for forty days, and one in `standard` that no process drives, stalled eleven
//! days. Each is built on a clock that starts that long before the design's run, and an
//! hour more, so it has waited that many days at every cut: the design's run ends two
//! hours after it starts. Both are older than every run a screen shows.
//!
//! Their workflow is fragua's first nodes in its two modes, under its name: what a field
//! and a run's screen read of either run (its handle, its mode, what holds it, how long)
//! is what the design's rows say, and neither runs past its first node.

use chrono::{DateTime, TimeDelta, Utc};
use yunta_testkit::MOCK_CONFIG;

use super::{id, started_by_a_crash, woken, SmallRun, NO_SESSIONS};
use crate::designed::place::Place;

/// fragua's first nodes, under its name, in its two modes: `quick` leaves `spec` out, as
/// fragua's own does.
const FRAGUA: &str = r#"
name: fragua
modes:
  quick:    { include: [grill, brief, plan, implement] }
  standard: { include: all }
nodes:
  - id: grill
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Find what only a person can decide."
  - id: brief
    kind: prompt
    runner: planner
    depends_on: [grill]
    prompt: "Write the brief."
  - id: plan
    kind: prompt
    runner: planner
    depends_on: [brief]
    prompt: "Register a tasks document."
  - id: spec
    kind: prompt
    runner: planner
    depends_on: [plan]
    prompt: "Write the tests the plan's tasks are held to."
  - id: implement
    kind: prompt
    runner: executor
    depends_on: [spec]
    prompt: "Build your task as its plan declares it."
"#;

/// `grill`'s session, refused by a CLI nobody logged in to, in the words Claude Code
/// says it in.
const NOT_LOGGED_IN: &str = r#"
dialect: claude-code
sessions:
  - outcome: { type: failed, message: "Not logged in · Please run /login" }
"#;

/// How many days the run that needs the person has waited when the design's run
/// starts: what the design's field shows it waiting.
const NEEDS_YOU_FOR_DAYS: i64 = 40;

/// How many days the stalled run has stood when the design's run starts: what the
/// design's field shows it standing.
const STALLED_FOR_DAYS: i64 = 11;

/// The hours each run starts earlier than its days say, so it has waited those days and
/// not one more at every cut, the design's run's last two hours after its first.
const AN_HOUR_MORE: i64 = 1;

/// A run of fragua in `quick` whose `grill` session a CLI nobody logged in to refused, 40
/// days before the design's run: no one at its terminal answers, so it parks on the
/// failure's decision.
pub async fn not_logged_in() -> SmallRun {
    let start = before_the_design(NEEDS_YOU_FOR_DAYS);
    let place = Place::starting(start, &id("EZRPE9"), &[], |bench| bench.in_mode("quick"));
    woken(&place, FRAGUA, NOT_LOGGED_IN, MOCK_CONFIG).await;
    SmallRun::of(place)
}

/// A run of fragua in `standard` whose engine started `grill` and died, 11 days before the
/// design's run: the log says `grill` runs, and no process drives the run once its
/// registry names a gone engine.
pub async fn long_stalled() -> SmallRun {
    let start = before_the_design(STALLED_FOR_DAYS);
    let place = Place::starting(start, &id("EKPBGE"), &[], |bench| bench.in_mode("standard"));
    place.bench.create(FRAGUA, NO_SESSIONS, MOCK_CONFIG).await;
    place.moves(1);
    started_by_a_crash(&place, "grill");
    SmallRun::of(place)
}

/// The instant `days` days and [`AN_HOUR_MORE`] before the design's run starts.
fn before_the_design(days: i64) -> DateTime<Utc> {
    yunta_testkit_core::fixed_now() - TimeDelta::days(days) - TimeDelta::hours(AN_HOUR_MORE)
}
