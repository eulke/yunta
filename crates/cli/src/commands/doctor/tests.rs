use surco::components::{Check, Checklist, Fix, Found};
use surco::look::Look;
use yunta_core::AdapterId;

use super::{kept_check, versioned, DoctorAnswer};
use crate::output::draw;
use crate::output::Answer;
use crate::output::Outcome;

/// What a machine ready to run finds: two checks that hold, and a first run
/// worth a look, with what that means and how to make it hold.
fn ready() -> Checklist {
    let mut checks = Checklist::default();
    checks.push_check(
        Check::new(
            Found::Holds,
            "claude-code",
            "healthy (2.1.305 (Claude Code))",
        )
        .brief("claude-code 2.1.305"),
    );
    checks.push(Found::Holds, "git", "commits as Ada <ada@example.com>");
    checks.push_check(
        Check::new(
            Found::Caution,
            "first run",
            "a new binary's first run takes 337 ms here, its next 4 ms",
        )
        .so("macOS assesses each new executable before its first run")
        .fix(Fix::said("add the terminal under Developer Tools")),
    );
    checks
}

#[test]
fn a_glance_folds_what_holds_and_says_how_to_make_a_caution_hold() {
    let answer = DoctorAnswer::of(false, false, &ready());
    let furrow = ".".repeat(80);
    assert_eq!(
        draw(answer.document(&Look::plain()), &Look::plain()),
        format!(
            "+ yunta doctor ; ready ; 1 thing worth a look\n{furrow}\n\n  \
             + 2 healthy  claude-code 2.1.305 ; git\n\n  \
             ^ first run  a new binary's first run takes 337 ms here, its next 4 ms\n    \
             so         macOS assesses each new executable before its first run\n    \
             fix        add the terminal under Developer Tools\n\n\
             next\n  yunta doctor --session   opens a session per binding, a prompt each\n  \
             yunta doctor --full      every check on its own row, with what it found\n"
        )
    );
    let json = serde_json::to_value(&answer).expect("the answer serializes");
    assert_eq!(
        json["checks"][2]["fix"]["said"],
        "add the terminal under Developer Tools"
    );
    assert!(json["checks"][0].get("brief").is_none(), "{json}");
}

#[test]
fn the_whole_list_gives_every_check_its_own_row() {
    let answer = DoctorAnswer::of(false, true, &ready());
    let page = draw(answer.document(&Look::plain()), &Look::plain());
    assert!(
        page.contains("\n  + claude-code  healthy (2.1.305 (Claude Code))\n"),
        "{page}"
    );
    assert!(
        page.contains("\n  + git          commits as Ada <ada@example.com>\n"),
        "{page}"
    );
    assert!(!page.contains("2 healthy"), "{page}");
}

#[test]
fn a_problem_makes_the_machine_not_ready_and_is_counted() {
    let mut checks = ready();
    checks.push(
        Found::Problem,
        "forge",
        "github acme/web: `TOKEN`, the variable its token is in, is not set",
    );
    let answer = DoctorAnswer::of(false, false, &checks);
    let page = draw(answer.document(&Look::plain()), &Look::plain());
    assert!(
        page.starts_with("! yunta doctor ; not ready ; 1 problem ; 1 thing worth a look\n"),
        "{page}"
    );
    assert_eq!(answer.outcome(), Outcome::Reported);
}

#[test]
fn a_healthy_adapter_folds_as_its_id_and_the_version_it_reported() {
    let claude = AdapterId::from_static("claude-code");
    assert_eq!(
        versioned(&claude, Some("2.1.305 (Claude Code)")),
        "claude-code 2.1.305"
    );
    assert_eq!(
        versioned(&AdapterId::from_static("codex"), Some("codex-cli 0.157.0")),
        "codex 0.157.0"
    );
    assert_eq!(versioned(&claude, None), "claude-code");
}

#[test]
fn the_whole_list_offers_no_list_to_unfold() {
    let answer = DoctorAnswer::of(false, true, &ready());
    let page = draw(answer.document(&Look::plain()), &Look::plain());
    assert!(!page.contains("--full"), "{page}");
}

/// What the checkouts keep, with `available` free of a disk that keeps
/// `keep_free`, and builds of `builds` within a `budget`.
fn kept(builds: u64, budget: u64, available: u64, keep_free: u64) -> yunta_engine::PoolReport {
    yunta_engine::PoolReport {
        checkouts: 2,
        builds,
        uncounted: 0,
        budget,
        available,
        keep_free,
    }
}

#[test]
fn checkouts_within_their_budget_hold() {
    let check = kept_check(&kept(10, 100, 500, 50));
    assert_eq!(check.found, Found::Holds);
    assert!(check.fix.is_none(), "{check:?}");
}

#[test]
fn builds_past_their_budget_are_worth_a_look_and_say_how_to_empty_them() {
    let check = kept_check(&kept(200, 100, 500, 50));
    assert_eq!(check.found, Found::Caution);
    assert_eq!(
        check.so.as_deref(),
        Some("the next checkout taken empties the builds past the budget")
    );
    assert_eq!(
        check.fix.map(|fix| fix.said).as_deref(),
        Some("run `yunta gc` to empty them now")
    );
}

#[test]
fn a_disk_under_what_it_keeps_free_stops_a_run_and_says_how_to_make_room() {
    let check = kept_check(&kept(10, 100, 40, 50));
    assert_eq!(check.found, Found::Problem);
    assert_eq!(
        check.fix.map(|fix| fix.said).as_deref(),
        Some("make room (`yunta gc`, close a parked run, or lower `storage.pool.keep_free`)")
    );
}
