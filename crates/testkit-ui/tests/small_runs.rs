//! Each small run, built twice, writes the same log, and stands where its scene needs:
//! its phase, the pause on its log and the menu the engine rebuilds for it, each held
//! here by name. The past runs stand as three finished runs of one workflow a new run
//! reads what it usually takes from.

#[path = "small_runs/asking.rs"]
mod asking;
#[path = "small_runs/beside.rs"]
mod beside;
#[path = "small_runs/ends.rs"]
mod ends;
#[path = "small_runs/past.rs"]
mod past;
#[path = "small_runs/standstills.rs"]
mod standstills;
#[path = "small_runs/waiting.rs"]
mod waiting;
#[path = "small_runs/work.rs"]
mod work;

use std::future::Future;

use yunta_core::events::{Menu, PauseReason, StoredEvent};
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_testkit_ui::small_runs::SmallRun;

/// Where a small run stands, as a reader of its log reads it.
struct Stand {
    frame: RunFrame,
    /// The pause standing last on the log, when the run is parked.
    pause: Option<PauseReason>,
    /// The menu of the decision the engine rebuilds for the run, when it rebuilds one.
    menu: Option<Menu>,
}

/// `build`'s run, built twice: the two logs are held equal, and the first is handed back.
async fn built_twice<F, Fut>(build: F) -> SmallRun
where
    F: Fn() -> Fut,
    Fut: Future<Output = SmallRun>,
{
    let first = build().await;
    let second = build().await;
    let (once, again) = (first.log(), second.log());
    let apart = once
        .lines()
        .zip(again.lines())
        .enumerate()
        .find(|(_, (one, other))| one != other);
    if let Some((line, (one, other))) = apart {
        let from = one
            .chars()
            .zip(other.chars())
            .take_while(|(a, b)| a == b)
            .count()
            .saturating_sub(40);
        let tail = |text: &str| text.chars().skip(from).take(160).collect::<String>();
        panic!(
            "`{}` built twice writes two logs, apart at line {line}:\n{}\n{}",
            first.run(),
            tail(one),
            tail(other)
        );
    }
    assert_eq!(
        once.lines().count(),
        again.lines().count(),
        "{}",
        first.run()
    );
    first
}

/// Where `run` stands: its frame, the pause on its log, and the menu rebuilt for it.
async fn stand(run: &SmallRun) -> Stand {
    let workspace = run.workspace();
    let project = workspace.project().expect("the small run's repository");
    let reading = project
        .run(run.run())
        .await
        .expect("the small run, opened")
        .reading()
        .await
        .expect("the small run, read");
    let menu = reading
        .waits_on()
        .and_then(|(_, escalation)| escalation.menu().cloned());
    Stand {
        pause: reading
            .frame
            .phase
            .waiting_on()
            .and_then(|on| on.reason())
            .cloned(),
        frame: reading.frame,
        menu,
    }
}

/// The kinds of `events`, in log order, each with the node it names.
fn kinds(events: &[StoredEvent]) -> Vec<(String, Option<String>)> {
    events
        .iter()
        .map(|event| {
            let node = event.node_id.as_ref().map(|node| node.as_str().to_string());
            (event.body.kind_name().to_string(), node)
        })
        .collect()
}

/// Where in `events` the first event of `kind` naming `node` stands, from `after` on.
fn at(events: &[StoredEvent], kind: &str, node: Option<&str>, after: usize) -> usize {
    kinds(events)
        .iter()
        .enumerate()
        .skip(after)
        .find(|(_, (each, named))| each == kind && named.as_deref() == node)
        .map(|(at, _)| at)
        .unwrap_or_else(|| panic!("no `{kind}` of {node:?} after {after}: {:?}", kinds(events)))
}

/// Whether `phase` is a run parked on a person or on something it waits for.
fn waiting(phase: &RunPhase) -> bool {
    phase.waiting_on().is_some()
}
