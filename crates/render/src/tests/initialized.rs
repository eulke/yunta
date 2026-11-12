//! What `init` ends with, in every environment: its runners proposed, as a
//! non-interactive `init` writes them; every one declared, as a person answered;
//! and some declared with the rest still proposed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::{AdapterId, Detected, ModelName, ProbedAdapter, RunnerCandidate, RunnerName};

use crate::page::{Initialized, Page};

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

/// `runner` on Claude Code, with `model`.
fn on_claude(model: &str) -> RunnerCandidate {
    RunnerCandidate {
        adapter: AdapterId::from_static("claude-code"),
        model: model.parse::<ModelName>().expect("a model name"),
        agent: None,
        effort: None,
    }
}

#[test]
fn what_init_ends_with_matches_its_goldens_in_every_environment() {
    let planner = RunnerName::from_static("planner");
    let cases = [
        ("proposed", BTreeMap::new(), vec![RunnerName::PROPOSED]),
        (
            "declared",
            BTreeMap::from([(RunnerName::PROPOSED, on_claude("opus"))]),
            Vec::new(),
        ),
        (
            "partly-declared",
            BTreeMap::from([(planner, on_claude("opus"))]),
            vec![RunnerName::PROPOSED],
        ),
    ];
    let (wrote, skipped) = (vec![".yunta/config.yaml".to_string()], Vec::new());
    let (detected, probed) = (Detected::default(), probed());
    for (name, declared, undeclared) in &cases {
        let page = Initialized {
            repo: Path::new("/home/me/project"),
            wrote: &wrote,
            skipped: &skipped,
            detected: &detected,
            probed: &probed,
            declared,
            undeclared,
        };
        for environment in &ENVIRONMENTS {
            let look = Look::of(environment);
            let drawn = Terminal::on(look).draw(&page.document(&look));
            assert_golden(&environment.golden(&goldens(), name), &drawn);
        }
    }
}
