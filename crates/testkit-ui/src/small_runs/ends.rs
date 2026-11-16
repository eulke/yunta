//! The small runs at either end of a run's life: one created that started no
//! node, one finished reporting a finding that blocks, one closed failed, one
//! closed for a successor in a later mode, and one whose log stops making
//! sense.

use yunta_core::events::{EventPayload, NodeEvent, NodeFinishedPayload, NodeOutcome, TokenUsage};
use yunta_testkit::{ScriptedInteraction, MOCK_CONFIG};

use super::{appended, id, woken, SmallRun, NO_SESSIONS};
use crate::designed::place::Place;
use crate::designed::script::{self, Act, Step, OPENED};

/// A command, then another after it.
const TWO_STEPS: &str = "name: two-steps\nnodes:\n  - { id: build, kind: bash, run: \"true\" }\n  \
                         - { id: ship, kind: bash, run: \"true\", depends_on: [build] }\n";

/// A run created and never woken: what runs first is all it says.
pub async fn created() -> SmallRun {
    let place = Place::of(&id("CREATD"), &[], |bench| bench);
    place
        .bench
        .create(TWO_STEPS, NO_SESSIONS, MOCK_CONFIG)
        .await;
    place.moves(30);
    SmallRun::of(place)
}

/// A review of the change, which posts what it finds.
const REVIEWS: &str = r#"
name: reviewed
nodes:
  - id: review
    kind: prompt
    runner: executor
    permissions: read-only
    prompt: "Review the change."
    artifacts:
      produces: [findings]
"#;

/// The reviewer's session: it reads, then posts a finding that blocks.
const FINDS_A_BUG: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: awaits_word, word: "@words@/review" }
      - { type: file_call, action: read, path: src/lib.rs }
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: bug
          severity: blocking
          title: "The lock is taken twice"
          location: "src/lib.rs:12"
          detail: "A second caller deadlocks on it."
    outcome: { type: completed, summary: reviewed }
"#;

/// The review at work, as long as a reviewer reads.
const REVIEWING: &[Act] = &[
    Act::Logged(OPENED, Some("review"), 1),
    Act::Moves(130),
    Act::Gives("review"),
];

/// A run that finished with a finding that blocks standing: it reports it.
pub async fn reported() -> SmallRun {
    let place = Place::of(&id("REP0RT"), &[], |bench| bench);
    place
        .bench
        .create(REVIEWS, &place.fixture(FINDS_A_BUG), MOCK_CONFIG)
        .await;
    script::played(&place, &[Step::Moves(2), Step::Wakes(REVIEWING)]).await;
    SmallRun::of(place)
}

/// A command that fails, saying why, until a file it looks for stands.
const FAILS_SAYING_WHY: &str = "name: aborts\nnodes:\n  - id: build\n    kind: bash\n    \
                                run: \"echo 'fixed.txt is missing' >&2; test -f fixed.txt\"\n";

/// The executor on the mock, and a failure that ends the run rather than
/// parking it.
const ENDS_ON_FAILURE: &str = "runners:\n  executor:\n    - { adapter: mock, model: mock-model }\n\
                               defaults:\n  on_failure: abort\n";

/// A run whose command fails where its repository ends a run on a failure: it
/// closes failed, the failure and what the command said on its log. A person's
/// `abort` on a failure's decision parks the run, as `asking::failure` does
/// before it; only `defaults.on_failure` closes one failed.
pub async fn failed() -> SmallRun {
    let place = Place::of(&id("DEFEAT"), &[], |bench| bench);
    woken(&place, FAILS_SAYING_WHY, NO_SESSIONS, ENDS_ON_FAILURE).await;
    SmallRun::of(place)
}

/// A check whose correction its mode leaves out, so its failure offers to
/// promote the run to the mode that holds it.
const PROMOTABLE: &str = r#"
name: promotable
modes:
  alpha:  { include: [lint, fix-lint] }
  beta:   { include: all }
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
"#;

/// A run whose failure a person at its terminal answers `promote`: it closes
/// for a successor in `beta`, created and not yet woken.
pub async fn promoted() -> SmallRun {
    let place = Place::of(&id("PR0M0T"), &[], |bench| bench.in_mode("alpha"));
    place
        .bench
        .create(PROMOTABLE, NO_SESSIONS, MOCK_CONFIG)
        .await;
    place.moves(2);
    place
        .bench
        .wake_answering(&ScriptedInteraction::choose("promote"))
        .await;
    place.moves(1);
    place.bench.successor("beta").await;
    SmallRun::of(place)
}

/// A gate nobody answers: its run parks on it.
const GATED: &str = yunta_testkit::workflows::GATED;

/// A run parked on its gate whose log then takes a node's close that names no
/// node, as a binary that wrote another log could append: the log stops making
/// sense there.
pub async fn broken() -> SmallRun {
    let place = Place::of(&id("BR0KEN"), &[], |bench| bench);
    woken(&place, GATED, NO_SESSIONS, MOCK_CONFIG).await;
    place.moves(50);
    let finished =
        NodeFinishedPayload::new(NodeOutcome::Summary("ok".into()), TokenUsage::default());
    let payload = EventPayload::Node(NodeEvent::Finished(finished));
    appended(&place, None, payload);
    SmallRun::of(place)
}
