//! What setting a project up found and would write, in every environment: on a machine
//! where an adapter answers, the runner its workflows propose; and on one where none
//! does.

use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::{AdapterId, Detected, ProbedAdapter, RunnerName};

use crate::page::{Page, Setup};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/setup")
}

/// Claude Code probed: answering where `healthy` says, not on the PATH otherwise.
fn claude(healthy: bool) -> ProbedAdapter {
    ProbedAdapter {
        id: AdapterId::from_static("claude-code"),
        healthy,
        detail: match healthy {
            true => "2.1.0".to_string(),
            false => "not on PATH".to_string(),
        },
    }
}

#[test]
fn what_setting_up_would_write_matches_its_goldens_in_every_environment() {
    let writes = vec![
        ".yunta/config.yaml".to_string(),
        ".gitignore".to_string(),
        ".claude/skills/yunta/SKILL.md".to_string(),
    ];
    let detected = Detected::default();
    let proposed = vec![RunnerName::PROPOSED];
    let cases = [
        ("answered", vec![claude(true)]),
        ("unanswered", vec![claude(false)]),
    ];
    for (name, probed) in &cases {
        let page = Setup {
            repo: Path::new("/home/me/project"),
            name: "project",
            base_branch: "main",
            detected: &detected,
            probed,
            proposed: &proposed,
            writes: &writes,
        };
        for environment in &ENVIRONMENTS {
            let look = Look::of(environment);
            let drawn = Terminal::on(look).draw(&page.document(&look));
            assert_golden(&environment.golden(&goldens(), name), &drawn);
        }
    }
}
