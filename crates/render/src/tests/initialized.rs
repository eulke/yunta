//! What `init` ends with, in every environment, each page one an input of `init`
//! produces: its runners proposed on a machine where no adapter answered; the stand-in
//! role declared, the default runner, with a model typed and with none; declared on two
//! adapters, in order; the roles a repository's workflows name, with no default; and in
//! a Rust repository on GitHub, its language and its forge said as a setup says them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::{
    AdapterId, CommandName, Detected, Ecosystem, GitHubRepo, ModelName, ProbedAdapter,
    RunnerCandidate, RunnerName,
};

use crate::page::{Initialized, Page, WrittenRunners};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/initialized")
}

/// Claude Code answered on this machine, and Codex did not.
fn probed() -> Vec<ProbedAdapter> {
    vec![
        ProbedAdapter {
            id: AdapterId::from_static("claude-code"),
            healthy: true,
            detail: "2.1.0".to_string(),
        },
        ProbedAdapter {
            id: AdapterId::from_static("codex"),
            healthy: false,
            detail: "not on PATH".to_string(),
        },
    ]
}

/// Claude Code, then Codex, answered on this machine.
fn both_probed() -> Vec<ProbedAdapter> {
    let mut probed = probed();
    probed[1].healthy = true;
    probed[1].detail = "0.157.0".to_string();
    probed
}

/// Neither Claude Code nor Codex answered on this machine.
fn none_probed() -> Vec<ProbedAdapter> {
    let mut probed = probed();
    probed[0].healthy = false;
    probed[0].detail = "not on PATH".to_string();
    probed
}

/// A runner's candidate on `adapter`, with `model` where one is given.
fn on(adapter: &'static str, model: Option<&str>) -> RunnerCandidate {
    RunnerCandidate {
        adapter: AdapterId::from_static(adapter),
        model: model.map(|model| model.parse::<ModelName>().expect("a model name")),
        agent: None,
        effort: None,
    }
}

/// A runner declared on Claude Code alone, with `model` where one is given.
fn on_claude(model: Option<&str>) -> Vec<RunnerCandidate> {
    vec![on("claude-code", model)]
}

/// A Rust repository whose `origin` is on GitHub, as detection finds it: its lint, its
/// suite and its formatter written as project commands.
fn rust_on_github() -> Detected {
    let command = |name: &'static str, text: &str| (CommandName::from_static(name), text.into());
    Detected {
        ecosystem: Some(Ecosystem {
            language: "rust",
            cache_tip: "share a build cache across worktrees",
        }),
        commands: BTreeMap::from([
            command("lint", "cargo clippy --all-targets -- -D warnings"),
            command("test", "cargo test"),
            command("format", "cargo fmt --check"),
        ]),
        suite: Some("cargo test".to_string()),
        forge: Some("eulke/yunta".parse::<GitHubRepo>().expect("a repository")),
        adapters: Default::default(),
    }
}

/// A page `init` ends with, by the golden it is held to: the runners it declared, or
/// none and those it proposed, the default runner, the adapters this machine answered
/// for, and what the repository was found to have.
struct Case {
    name: &'static str,
    declared: Option<Vec<(RunnerName, Vec<RunnerCandidate>)>>,
    proposed: Vec<RunnerName>,
    default: Option<RunnerName>,
    probed: Vec<ProbedAdapter>,
    detected: Detected,
}

/// The page `name` of a repository with no workflow and nothing found, its stand-in role
/// the default runner: declared on `candidates` where some are given, proposed otherwise.
fn stand_in(
    name: &'static str,
    candidates: Vec<RunnerCandidate>,
    probed: Vec<ProbedAdapter>,
) -> Case {
    let declared = (!candidates.is_empty()).then(|| vec![(RunnerName::PROPOSED, candidates)]);
    Case {
        name,
        declared,
        proposed: vec![RunnerName::PROPOSED],
        default: Some(RunnerName::PROPOSED),
        probed,
        detected: Detected::default(),
    }
}

/// Every page held. Each is what `init` ends with for an input: no adapter answering
/// (`proposed`); Claude Code alone answering, `opus` typed (`declared`) or nothing
/// asked (`declared-on-its-adapter`); Claude Code then Codex answering, Claude Code
/// picked with `opus` (`declared-in-order`); workflows naming `planner` and `executor`
/// on every node, nothing asked (`named-roles`); and a Rust repository on GitHub with
/// `opus` typed (`rust-on-github`).
fn cases() -> Vec<Case> {
    let in_order = vec![on("claude-code", Some("opus")), on("codex", None)];
    let named =
        ["planner", "executor"].map(|runner| (RunnerName::from_static(runner), on_claude(None)));
    vec![
        stand_in("proposed", Vec::new(), none_probed()),
        stand_in("declared", on_claude(Some("opus")), probed()),
        stand_in("declared-on-its-adapter", on_claude(None), probed()),
        stand_in("declared-in-order", in_order, both_probed()),
        Case {
            name: "named-roles",
            proposed: named.iter().map(|(runner, _)| runner.clone()).collect(),
            declared: Some(named.to_vec()),
            default: None,
            ..stand_in("named-roles", Vec::new(), probed())
        },
        Case {
            detected: rust_on_github(),
            ..stand_in("rust-on-github", on_claude(Some("opus")), probed())
        },
    ]
}

#[test]
fn what_init_ends_with_matches_its_goldens_in_every_environment() {
    let (wrote, skipped) = (vec![".yunta/config.yaml".to_string()], Vec::new());
    for case in cases() {
        let runners = match &case.declared {
            Some(declared) => WrittenRunners::Declared(
                (declared.iter())
                    .map(|(runner, candidates)| (runner, candidates.as_slice()))
                    .collect(),
            ),
            None => WrittenRunners::Proposed(&case.proposed),
        };
        let page = Initialized {
            repo: Path::new("/home/me/project"),
            wrote: &wrote,
            skipped: &skipped,
            detected: &case.detected,
            probed: &case.probed,
            runners,
            default: case.default.as_ref(),
        };
        for environment in &ENVIRONMENTS {
            let look = Look::of(environment);
            let drawn = Terminal::on(look).draw(&page.document(&look));
            assert_golden(&environment.golden(&goldens(), case.name), &drawn);
        }
    }
}

/// What `init` declared says each candidate in one form, a setup's row's: on its adapter, its
/// model after a slash where one was typed, the first as the others; here the default runner
/// on Claude Code with `opus` typed, then Codex with `sonnet`.
#[test]
fn init_says_every_candidate_in_one_form() {
    let candidates = vec![on("claude-code", Some("opus")), on("codex", Some("sonnet"))];
    let case = stand_in("in-one-form", candidates, both_probed());
    let declared = case.declared.as_ref().expect("a runner declared");
    let (wrote, skipped) = (vec![".yunta/config.yaml".to_string()], Vec::new());
    let page = Initialized {
        repo: Path::new("/home/me/project"),
        wrote: &wrote,
        skipped: &skipped,
        detected: &case.detected,
        probed: &case.probed,
        runners: WrittenRunners::Declared(
            (declared.iter())
                .map(|(runner, candidates)| (runner, candidates.as_slice()))
                .collect(),
        ),
        default: case.default.as_ref(),
    };
    let drawn = surco::surface::Markdown.draw(&page.document(&surco::surface::Markdown.look()));
    assert!(
        drawn.contains("implementer (the default) on claude-code/opus, then codex/sonnet"),
        "{drawn}"
    );
}
