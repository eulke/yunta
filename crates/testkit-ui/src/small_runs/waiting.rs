//! The small runs that wait on something other than a person here: a forge's
//! rate limit to reset, a forge's reviewers, and a child run parked on its own
//! gate.

use yunta_testkit::MOCK_CONFIG;

use super::stand_ins::{forge, Limited, Reviewing};
use super::{id, woken, SmallRun, NO_SESSIONS, PUBLISHES};
use crate::designed::place::Place;

/// The forge the runs' repository names.
const FORGE: &str = "forge:\n  github: { repo: acme/web, token_env: ACME_TOKEN }\n";

/// A change, then its pull request.
const WRITES_THEN_OPENS: &str = r#"
name: opens
nodes:
  - { id: make, kind: bash, run: "echo made > made.txt" }
  - { id: pr, kind: pull_request, depends_on: [make], title: "add dark mode", body: "What the run did." }
"#;

/// A run whose pull request the forge refuses until its rate limit resets:
/// the run parks until then, and goes on by itself.
pub async fn wakes_alone() -> SmallRun {
    let place = Place::of(&id("RESETS"), &[], |bench| bench.with_forge(forge(Limited)));
    let config = format!("{MOCK_CONFIG}{FORGE}");
    woken(&place, WRITES_THEN_OPENS, NO_SESSIONS, &config).await;
    SmallRun::of(place)
}

/// A gate published as a pull request a forge's reviewers have not answered:
/// the run parks on the forge.
pub async fn pull_request() -> SmallRun {
    let place = Place::of(&id("PR0PEN"), &[], |bench| {
        bench.with_forge(forge(Reviewing::default()))
    });
    let config = format!("{MOCK_CONFIG}{FORGE}");
    woken(&place, PUBLISHES, NO_SESSIONS, &config).await;
    SmallRun::of(place)
}

/// The child: a gate, then a command once it is passed.
const GATED: &str = r#"
name: gated
nodes:
  - id: approve
    kind: gate
    assignee: lead
  - id: work
    kind: bash
    depends_on: [approve]
    run: "echo done > child-out.txt"
"#;

/// The parent, whose one node runs the child in its own checkout.
const PARENT: &str = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: gated
    isolation: none
"#;

/// A parent whose child parks on its gate, with no one at the terminal: the
/// parent parks on its child.
pub async fn child() -> SmallRun {
    let files = [(".yunta/workflows/gated.yaml", GATED)];
    let place = Place::of(&id("PARENT"), &files, |bench| bench);
    woken(&place, PARENT, NO_SESSIONS, MOCK_CONFIG).await;
    SmallRun::of(place)
}
