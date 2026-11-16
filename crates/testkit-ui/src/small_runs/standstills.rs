//! The small runs that stand with nothing a person picks to answer them: a
//! pause whose escalation carries no menu, no room on the disk, a node left
//! half done, a run blocked, a run stopped, a gate aborted, a run its engine
//! left mid-node, and a gate asked at the terminal of the engine that drives
//! it.
//!
//! What no wake of this binary writes, an older binary's log or a crash, is
//! appended by hand as the engine writes it, as the engine's own tests write
//! a crash.

use yunta_api::Stop;
use yunta_core::events::{
    EventPayload, PauseReason, RunEvent, RunPausedPayload, RunResumedPayload,
};
use yunta_core::NodeId;
use yunta_testkit::MOCK_CONFIG;

use super::stand_ins::AtTheTerminal;
use super::{
    appended, id, started_by_a_crash, woken, woken_alone, SmallRun, NO_SESSIONS, PUBLISHES,
};
use crate::designed::place::Place;
use crate::designed::script::{self, Act, Step};

/// A gate on a pull request parked at the console, woken again by an older
/// binary that parked it on an escalation with no menu, as its log keeps one:
/// the pause reads in the log's words, and no option answers it here.
pub async fn no_menu() -> SmallRun {
    let place = woken_alone("N0MENV", PUBLISHES, NO_SESSIONS, MOCK_CONFIG).await;
    let pause = place
        .bench
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Run(RunEvent::Paused(paused))) => Some(paused.pause.clone()),
            _ => None,
        })
        .expect("the gate parked the run");
    place.moves(600);
    appended(
        &place,
        None,
        EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None))),
    );
    appended(
        &place,
        None,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload {
            pause: menuless(&pause),
        })),
    );
    SmallRun::of(place)
}

/// `pause`, its escalation's menu left out, as a log older than menus keeps it.
fn menuless(pause: &PauseReason) -> PauseReason {
    let mut written = serde_json::to_value(pause).expect("a pause as its log keeps it");
    let fields = written.as_object_mut().expect("a pause is an object");
    for menu in [
        "gate",
        "failure",
        "departure",
        "scope",
        "budget",
        "loop_cap",
        "console",
        "published",
    ] {
        fields.remove(menu);
    }
    serde_json::from_value(written).expect("a pause with no menu reads")
}

/// One command.
const ONE_STEP: &str = "name: one-step\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n";

/// A run that has to keep more of the disk free than any disk holds: it parks
/// before its first node, saying what is free against what it needs.
pub async fn no_room() -> SmallRun {
    let config = format!("{MOCK_CONFIG}storage:\n  pool:\n    keep_free: \"9000T\"\n");
    let place = woken_alone("N0R00M", ONE_STEP, NO_SESSIONS, &config).await;
    SmallRun::of(place)
}

/// A command whose effect is not safe to repeat.
const UNCERTAIN: &str = "name: uncertain-on-crash\nnodes:\n  - id: only\n    kind: bash\n    \
                         run: \"test -f present.txt\"\n    on_interrupt: fail_if_uncertain\n";

/// A run whose engine died while its one command ran, which may not run
/// again unchecked: the wake that finds it parks on the node left half done.
pub async fn orphans() -> SmallRun {
    let place = Place::of(&id("0RPHAN"), &[], |bench| bench);
    place
        .bench
        .create(UNCERTAIN, NO_SESSIONS, MOCK_CONFIG)
        .await;
    place.moves(1);
    started_by_a_crash(&place, "only");
    script::played(&place, &[Step::Moves(900), Step::Wakes(&[])]).await;
    SmallRun::of(place)
}

/// A command that fails, and one that waits on it.
const FAILS_FIRST: &str = "name: blocked\nnodes:\n  - id: build\n    kind: bash\n    \
                           run: \"test -f fixed.txt\"\n  - id: ship\n    kind: bash\n    \
                           run: \"true\"\n    depends_on: [build]\n";

/// A command failed and parked on its decision, woken again by a binary that
/// found nothing runnable: `ship` waits on `build`, which nothing resolves.
pub async fn blocked() -> SmallRun {
    let place = woken_alone("B10CKD", FAILS_FIRST, NO_SESSIONS, MOCK_CONFIG).await;
    place.moves(300);
    appended(
        &place,
        None,
        EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None))),
    );
    let blocked = PauseReason::Blocked {
        node: NodeId::from("ship"),
        on: vec![NodeId::from("build")],
    };
    appended(
        &place,
        None,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload { pause: blocked })),
    );
    SmallRun::of(place)
}

/// A command that runs until it is stopped: the lint, held by the `cargo` the
/// place puts first on the run's `PATH`.
const LINTS: &str = "name: cancel-me\nnodes:\n  - id: lint\n    kind: bash\n    \
                     run: \"cargo clippy --workspace --all-targets\"\n";

/// A run a person stopped while its command worked: it parks cancelled.
pub async fn stopped() -> SmallRun {
    let stop = Stop::new();
    let place = Place::of(&id("ST0PPD"), &[], |bench| {
        bench.with_cancel(stop.work().clone())
    });
    place.bench.create(LINTS, NO_SESSIONS, MOCK_CONFIG).await;
    place.moves(2);
    place.arm("clippy");
    let working = [Act::Held("clippy"), Act::Moves(45)];
    let (_report, ()) = tokio::join!(place.bench.wake(), async {
        script::acted(&place, &working).await;
        stop.stop();
    });
    SmallRun::of(place)
}

/// A check whose correction cannot fix it, its one reroute spent.
const HOPELESS: &str = r#"
name: hopeless-until-retried
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "Try to fix it."
"#;

/// The correction's session, which fixes nothing.
const FIXES_NOTHING: &str = "dialect: claude-code\nsessions:\n  - outcome: { type: completed, \
                             summary: \"did nothing useful\" }\n";

/// A check sent back to its correction, which fails again with its reroute
/// spent, and a person who answers its decision `abort`, with why: the run
/// parks where it is.
pub async fn aborted() -> SmallRun {
    let place = Place::of(&id("AB0RTD"), &[], |bench| bench);
    woken(&place, HOPELESS, FIXES_NOTHING, MOCK_CONFIG).await;
    place.moves(240);
    place
        .bench
        .answer_parked("abort", Some("not worth chasing today"))
        .await
        .expect("the decision takes `abort`");
    script::played(&place, &[Step::Moves(5), Step::Wakes(&[])]).await;
    SmallRun::of(place)
}

/// A command, then another after it.
const TWO_STEPS: &str = "name: two-steps\nnodes:\n  - { id: build, kind: bash, run: \"true\" }\n  \
                         - { id: ship, kind: bash, run: \"true\", depends_on: [build] }\n";

/// A run whose engine started its first command and died: the log says the
/// command runs, and no process drives the run once its registry names a gone
/// engine.
pub async fn stalled() -> SmallRun {
    let place = Place::of(&id("STA11D"), &[], |bench| bench);
    place
        .bench
        .create(TWO_STEPS, NO_SESSIONS, MOCK_CONFIG)
        .await;
    place.moves(1);
    started_by_a_crash(&place, "build");
    place.moves(420);
    SmallRun::of(place)
}

/// A command, then a gate.
const ASKS_TO_SHIP: &str = "name: asks\nnodes:\n  - { id: first, kind: bash, run: \"true\" }\n  \
                            - { id: ship, kind: gate, depends_on: [first], assignee: lead, \
                            message: \"Ship it?\", options: [approve] }\n";

/// A gate asked at the terminal of the engine that drives the run, and
/// answered there: cut before its answer, while a registry names that engine
/// asking, the question stands open at another terminal.
pub async fn asked_elsewhere() -> SmallRun {
    let place = Place::of(&id("ASKERS"), &[], |bench| bench);
    place
        .bench
        .create(ASKS_TO_SHIP, NO_SESSIONS, MOCK_CONFIG)
        .await;
    place.moves(2);
    let lead = AtTheTerminal {
        answers: &[],
        choice: Some("approve"),
    };
    place.bench.wake_answering(&lead).await;
    SmallRun::of(place)
}
