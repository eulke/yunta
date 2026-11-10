use std::time::Duration;

use surco::look::Look;
use yunta_core::{AdapterId, GitHubRepo};
use yunta_render::page::doctor::{AdapterHealth, Finding};
use yunta_render::page::Page;

use super::DoctorAnswer;
use crate::output::draw;
use crate::output::{Answer, Outcome};

/// What a machine ready to run finds: two checks that hold, and a first run
/// worth a look, with what that means and how to make it hold.
fn ready() -> Vec<Finding> {
    vec![
        Finding::Adapter {
            adapter: AdapterId::from_static("claude-code"),
            health: AdapterHealth::Healthy {
                version: Some("2.1.305 (Claude Code)".to_string()),
            },
        },
        Finding::Committer {
            who: "Ada <ada@example.com>".to_string(),
        },
        Finding::FirstRun {
            first: Duration::from_millis(337),
            then: Duration::from_millis(4),
            slows_a_suite: true,
        },
    ]
}

fn answer(findings: Vec<Finding>, full: bool) -> DoctorAnswer {
    DoctorAnswer {
        session: false,
        findings,
        full,
    }
}

#[test]
fn a_glance_folds_what_holds_and_says_how_to_make_a_caution_hold() {
    let answer = answer(ready(), false);
    let furrow = ".".repeat(80);
    assert_eq!(
        draw(answer.page().document(&Look::plain()), &Look::plain()),
        format!(
            "+ yunta doctor ; ready ; 1 thing worth a look\n{furrow}\n\n  \
             + 2 healthy  claude-code 2.1.305 ; git\n\n  \
             ^ first run  a new binary's first run takes 337 ms here, its next 4 ms\n    \
             so         macOS assesses each new executable before its first run, and a\n               \
             suite pays that for every test binary it links again\n    \
             fix        add the terminal that runs yunta under System Settings > Privacy\n               \
             & Security > Developer Tools, then open it again\n\n\
             next\n  yunta doctor --session   opens a session per binding, a prompt each\n  \
             yunta doctor --full      every check on its own row, with what it found\n"
        )
    );
    let json = serde_json::to_value(&answer).expect("the answer serializes");
    assert!(
        json["checks"][2]["fix"]["said"]
            .as_str()
            .is_some_and(|said| said.contains("Developer Tools")),
        "{json}"
    );
    assert!(json["checks"][0].get("brief").is_none(), "{json}");
}

#[test]
fn the_whole_list_gives_every_check_its_own_row() {
    let answer = answer(ready(), true);
    let page = draw(answer.page().document(&Look::plain()), &Look::plain());
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
    let mut findings = ready();
    findings.push(Finding::ForgeTokenUnset {
        repo: "acme/web".parse::<GitHubRepo>().unwrap(),
        token_env: "TOKEN".to_string(),
        opening: vec!["ship".to_string()],
    });
    let answer = answer(findings, false);
    let page = draw(answer.page().document(&Look::plain()), &Look::plain());
    assert!(
        page.starts_with("! yunta doctor ; not ready ; 1 problem ; 1 thing worth a look\n"),
        "{page}"
    );
    assert_eq!(answer.outcome(), Outcome::Reported);
}

#[test]
fn the_whole_list_offers_no_list_to_unfold() {
    let answer = answer(ready(), true);
    let page = draw(answer.page().document(&Look::plain()), &Look::plain());
    assert!(!page.contains("--full"), "{page}");
}
