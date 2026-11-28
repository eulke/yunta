//! A run at a glance is a block every surface draws: as Markdown, as a page, and on a terminal
//! in every environment.

use std::path::{Path, PathBuf};

use super::AtAGlance;
use crate::components::{Binding, Keys};
use crate::doc::Doc;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::look::Look;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/at_a_glance")
}

/// What can be done about a run, under the heading the field's glance gives them: `keys` and
/// what each does.
fn acts(keys: &[(&str, &str)]) -> Option<(String, Vec<Binding>)> {
    let acts = keys
        .iter()
        .map(|(keys, does)| Binding {
            keys: Keys::Typed(keys.to_string()),
            does: does.to_string(),
        })
        .collect();
    Some(("what you can do".to_string(), acts))
}

/// A run its engine asks at its terminal, as the field's glance says it: how far it came, what
/// it spent and how long it took, its slowest nodes and its branch, and the two acts it offers.
pub(crate) fn asking() -> AtAGlance {
    AtAGlance {
        facts: vec![
            ("progress", "12 of 15 nodes done, 1 reroute".to_string()),
            ("tokens", "26.1M in / 1.06M out (25M cached)".to_string()),
            ("time", "1h12m at work, 38m51s waiting on you".to_string()),
            (
                "slowest",
                "implement 41m05s, spec 13m53s, plan 6m25s".to_string(),
            ),
            ("branch", "yunta/run/...Q78GRR off main".to_string()),
        ],
        acts: acts(&[
            ("c", "stop it and everything under it"),
            ("s", "what it cost"),
        ]),
    }
}

/// A run waiting on a decision, as the field's glance says it: how long it has waited and its
/// branch, and `enter` among its acts.
fn waiting() -> AtAGlance {
    AtAGlance {
        facts: vec![
            ("waiting", "for 40d".to_string()),
            ("branch", "yunta/run/...EZRPE9 off main".to_string()),
        ],
        acts: acts(&[("enter", "read the decision"), ("x", "close it for good")]),
    }
}

#[test]
fn a_run_at_a_glance_matches_its_goldens_on_every_surface() {
    for (case, glance) in [("asking", asking()), ("waiting", waiting())] {
        let doc = Doc::new().with(glance);
        assert_golden(&goldens().join(format!("{case}.md")), &Markdown.draw(&doc));
        assert_golden(&goldens().join(format!("{case}.html")), &Html.body(&doc));
        for environment in ENVIRONMENTS.iter().chain(&RICH) {
            let drawn = Terminal::on(Look::of(environment)).draw(&doc);
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}
