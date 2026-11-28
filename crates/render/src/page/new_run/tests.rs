//! A new run's words: the hint under its request, which says how the workflow asks as the
//! chosen mode has its nodes ask, and nothing of it where none asks; where the chosen mode
//! stops for a person;
//! an input's label over its box; the legend by the row in focus; and the panel beside it, what
//! the workflow usually takes from three past runs on and what this machine answers for.

use std::path::Path;
use std::time::Duration;

use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::words::Standing;
use yunta_core::{AdapterId, ContentHash, InputName, ModeName, RunId, Workflow};
use yunta_replay::history::RunSummary;

use super::{NewRun, WhatItTakes};
use crate::page::doctor::{AdapterHealth, CheckoutsKeep, Finding, ForgeAnswer, PoolKept};
use crate::page::Page;
use crate::screen::{new_run_legend, NewRunRow};

/// A workflow with a request, a node that asks, two gates and a second input, and four
/// modes: one whose rounds stop only on a required question, one that runs everything, one
/// that leaves the node that asks out, and one with no gate.
const DECLARED: &str = r#"
name: fragua
description: the full cycle
inputs:
  idea:
    type: string
    request: true
    required: true
    description: "this becomes the brief"
  tasks:
    type: document
    kind: tasks
    required: true
    description: "Tasks document YAML, relative to the repository root."
modes:
  quick: { include: [grill, build, ship], ask: required }
  standard: { include: [grill, approve-plan, build, ship] }
  quiet: { include: [approve-plan, build, ship] }
  gateless: { include: [grill, build] }
nodes:
  - id: grill
    kind: prompt
    runner: planner
    prompt: "Ask what only a person can decide."
    artifacts:
      produces: [questions]
  - { id: approve-plan, kind: gate, assignee: lead, depends_on: [grill] }
  - { id: build, kind: bash, run: "true", depends_on: [approve-plan] }
  - { id: ship, kind: gate, assignee: lead, depends_on: [build] }
"#;

fn workflow() -> Workflow {
    yunta_core::workflow::read::read(DECLARED, Path::new("fragua.yaml")).expect("a workflow")
}

fn mode(name: &str) -> ModeName {
    name.parse().expect("a mode's name")
}

#[test]
fn it_names_each_gate_the_chosen_mode_stops_at() {
    let workflow = workflow();
    let standard = mode("standard");
    let new_run = NewRun {
        workflow: &workflow,
        mode: &standard,
    };
    let stops = new_run.stops(Glyphs::Unicode).map(|line| line.text());
    assert_eq!(
        stops.as_deref(),
        Some("◆ it stops for you at approve-plan and ship")
    );
    let ascii = new_run.stops(Glyphs::Ascii).map(|line| line.text());
    assert_eq!(
        ascii.as_deref(),
        Some("? it stops for you at approve-plan and ship")
    );
    let gateless = mode("gateless");
    let never = NewRun {
        workflow: &workflow,
        mode: &gateless,
    };
    assert_eq!(never.stops(Glyphs::Unicode).map(|line| line.text()), None);
    assert_eq!(
        drawn(&new_run),
        [
            "what runs",
            "grill approve build ship",
            "? it stops for you at approve-plan and ship"
        ]
    );
    assert_eq!(drawn(&never), ["what runs", "grill build"]);
}

/// What runs under `new_run`'s rows, in ASCII, as plain text: its heading, then what it says
/// under its strokes.
fn drawn(new_run: &NewRun) -> Vec<String> {
    let foot = new_run.what_runs(Glyphs::Ascii);
    std::iter::once(&foot.heading)
        .chain(&foot.said)
        .map(Line::text)
        .collect()
}

/// Fragua as its pack declares it.
fn fragua() -> Workflow {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/fragua/.yunta/workflows/fragua.yaml");
    let declared = std::fs::read_to_string(&path).expect("fragua's workflow");
    yunta_core::workflow::read::read(&declared, &path).expect("fragua reads")
}

/// For each of fragua's modes, what runs strikes a stroke for each of its fifteen nodes, left
/// out where the mode's `include` leaves the node out, and names the nodes it runs and only
/// those, as the ribbon over a run of fragua names them, then where it stops: `quick` leaves
/// out the spec, the plan's approval, both reviews and the findings' fixes and stops at `ship`;
/// `standard` and `full` run every node and stop at the plan's approval and at `ship`.
#[test]
fn fraguas_modes_each_run_the_nodes_their_include_lists_and_say_where_they_stop() {
    let workflow = fragua();
    let every = "grill brief plan spec approve implement review alt conform fix lint fix-lint \
                 tests ship pr";
    let stops_twice = "? it stops for you at approve-plan and ship";
    let cases = [
        (
            "quick",
            "...oo.oo.o.....",
            "grill brief plan implement conform lint fix-lint tests ship pr",
            "? it stops for you at ship",
        ),
        ("standard", "...............", every, stops_twice),
        ("full", "...............", every, stops_twice),
    ];
    for (name, strokes, named, stops) in cases {
        let chosen = mode(name);
        let new_run = NewRun {
            workflow: &workflow,
            mode: &chosen,
        };
        let foot = new_run.what_runs(Glyphs::Ascii);
        let drawn: String = foot
            .strokes
            .iter()
            .map(|step| match step.standing {
                Standing::Skipped => 'o',
                _ => '.',
            })
            .collect();
        assert_eq!(drawn, strokes, "{name}'s strokes");
        assert_eq!(drawn_said(&foot.said), [named, stops], "{name}");
    }
}

/// `said` as plain text, a line each.
fn drawn_said(said: &[Line]) -> Vec<String> {
    said.iter().map(Line::text).collect()
}

#[test]
fn the_hint_says_what_the_workflow_asks_as_the_chosen_mode_asks_it() {
    let workflow = workflow();
    let standard = mode("standard");
    let asking = NewRun {
        workflow: &workflow,
        mode: &standard,
    };
    assert_eq!(
        asking.hint().as_deref(),
        Some("this becomes the brief; fragua asks before it decides for you")
    );
    let quiet = mode("quiet");
    let never_asks = NewRun {
        workflow: &workflow,
        mode: &quiet,
    };
    assert_eq!(never_asks.hint().as_deref(), Some("this becomes the brief"));
    let quick = mode("quick");
    let asks_what_it_must = NewRun {
        workflow: &workflow,
        mode: &quick,
    };
    assert_eq!(
        asks_what_it_must.hint().as_deref(),
        Some("this becomes the brief; fragua asks only what it cannot decide for you")
    );
    let request = asking.request("add support", "");
    assert_eq!(request.asked.text(), "What do you want to build?");
    assert_eq!(
        request.placeholder.as_deref(),
        Some("add support for installing a pack globally")
    );
    assert_eq!(request.hint, asking.hint());
}

#[test]
fn an_input_is_labelled_by_its_name_and_kind_and_hinted_by_its_description() {
    let workflow = workflow();
    let tasks: InputName = "tasks".parse().expect("an input's name");
    let spec = workflow.inputs.get(&tasks).expect("the tasks input");
    assert_eq!(
        NewRun::input_label(&tasks, spec, true).text(),
        "tasks (document, required)"
    );
    let asked = NewRun::input(&tasks, spec, ("", ""), false);
    assert_eq!(asked.asked.text(), "tasks (document, required)");
    assert_eq!(
        asked.hint.as_deref(),
        Some("Tasks document YAML, relative to the repository root.")
    );
}

/// A workflow that reads no request says so under the request, so words typed there are never
/// taken for one; a row's heading is strong where the keys act on it and muted elsewhere.
#[test]
fn a_workflow_that_reads_no_request_says_so_under_it() {
    let declared = "name: lint-fix\nnodes:\n  - { id: lint, kind: bash, run: \"true\" }\n";
    let workflow =
        yunta_core::workflow::read::read(declared, Path::new("lint-fix.yaml")).expect("a workflow");
    let mode = workflow.first_mode();
    let new_run = NewRun {
        workflow: &workflow,
        mode: &mode,
    };
    assert_eq!(new_run.hint().as_deref(), Some("lint-fix takes no request"));
    let tone = |line: Line| line.spans().first().map(|span| span.tone);
    assert_eq!(
        tone(NewRun::heading(NewRun::MODE, true)),
        Some(Tone::Strong)
    );
    assert_eq!(
        tone(NewRun::heading(NewRun::MODE, false)),
        Some(Tone::Muted)
    );
}

#[test]
fn the_legend_says_what_the_keys_do_on_the_row_in_focus() {
    let workflow = workflow();
    let tasks: InputName = "tasks".parse().expect("an input's name");
    let spec = workflow.inputs.get(&tasks).expect("the tasks input");
    let spelled = |row: NewRunRow, glyphs: Glyphs| {
        crate::tests::screen::spelled_in(&new_run_legend(row), glyphs)
    };
    assert_eq!(
        spelled(NewRunRow::Request, Glyphs::Unicode),
        "type the request · tab next row · enter start · esc back"
    );
    assert_eq!(
        spelled(NewRunRow::Workflow, Glyphs::Unicode),
        "↑↓ workflow · tab next row · enter start · esc back"
    );
    assert_eq!(
        spelled(NewRunRow::Mode, Glyphs::Unicode),
        "←→ mode · tab next row · enter start · esc back"
    );
    assert_eq!(
        spelled(NewRunRow::Mode, Glyphs::Ascii),
        "left/right mode ; tab next row ; enter start ; esc back"
    );
    assert_eq!(
        spelled(NewRunRow::Input { name: &tasks, spec }, Glyphs::Unicode),
        "type the tasks document · tab next row · enter start · esc back"
    );
}

/// A past run of the workflow, the `n`th, that spent `tokens` and took `took` seconds.
fn past(n: u32, tokens: u64, took: u64) -> RunSummary {
    RunSummary {
        run_id: RunId::from(format!("01M7SMA11RVN0000000PAST{n:02}").as_str()),
        mode: mode("quick"),
        workflow_hash: ContentHash::sha256(b"fragua"),
        tokens,
        wall_clock: Some(Duration::from_secs(took)),
        tasks_total: 1,
        cptv: None,
    }
}

/// The past runs the design's repository holds, oldest first: the design's own run, then two
/// quick ones.
fn three_past_runs() -> Vec<RunSummary> {
    vec![
        past(0, 27_118_000, 6_749),
        past(1, 1_380_000, 2_820),
        past(2, 2_170_000, 4_020),
    ]
}

/// The rows `page` holds, as text under `look`: its lines, then each check as `{subject}  {said}`.
fn rows(page: &WhatItTakes, look: &Look) -> Vec<String> {
    let blocks = page.document(look).into_blocks();
    blocks
        .iter()
        .flat_map(|block| match block {
            Block::Lines(lines) => lines.iter().map(Line::text).collect(),
            Block::Checklist(checks) => checks
                .checks()
                .map(|check| format!("{}  {}", check.subject, check.said))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

fn unicode() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        ..Look::plain()
    }
}

#[test]
fn what_it_usually_takes_says_a_figure_only_from_three_past_runs() {
    let runs = three_past_runs();
    let two = WhatItTakes {
        past: Some(&runs[1..]),
        machine: None,
    };
    assert_eq!(
        rows(&two, &unicode()),
        ["what it usually takes", "2 past runs"]
    );
    let three = WhatItTakes {
        past: Some(&runs),
        machine: None,
    };
    assert_eq!(
        rows(&three, &unicode()),
        [
            "what it usually takes",
            "3 past runs",
            "median 2.17M tokens, p90 27.1M",
            "median wall-clock 1h07m",
            "█▁▂",
        ]
    );
    let unread = WhatItTakes {
        past: None,
        machine: None,
    };
    assert_eq!(rows(&unread, &unicode()), ["what it usually takes"]);
}

#[test]
fn the_forge_in_reach_reads_as_its_repository() {
    let reachable = [Finding::Forge {
        repo: "eulke/yunta".parse().expect("a repository"),
        answer: ForgeAnswer::Reachable,
    }];
    let panel = WhatItTakes {
        past: None,
        machine: Some(&reachable),
    };
    assert_eq!(
        rows(&panel, &unicode()),
        ["what it usually takes", "forge  github eulke/yunta"]
    );
}

/// What `doctor` finds of the design's machine with its sessions unopened: two adapters that
/// answer, a committer, a forge whose token is not set, checkouts within their budget, and a
/// binary's first run that slows a suite.
fn the_machine() -> Vec<Finding> {
    vec![
        Finding::Adapter {
            adapter: AdapterId::from_static("claude-code"),
            health: AdapterHealth::Healthy {
                version: Some("2.1.0 (Claude Code)".to_string()),
            },
            serving: vec!["yunta/fragua".to_string()],
        },
        Finding::Adapter {
            adapter: AdapterId::from_static("codex"),
            health: AdapterHealth::Healthy {
                version: Some("codex-cli 0.157.0".to_string()),
            },
            serving: vec!["yunta/fragua".to_string()],
        },
        Finding::Committer {
            name: "Marta Ruiz".to_string(),
            address: "marta.ruiz@example.com".to_string(),
        },
        Finding::ForgeTokenUnset {
            repo: "eulke/yunta".parse().expect("a repository"),
            token_env: "YUNTA_DESIGNED_FORGE_TOKEN".to_string(),
            opening: vec!["yunta/fragua".to_string()],
        },
        Finding::Checkouts {
            kept: PoolKept {
                checkouts: 0,
                builds: 0,
                uncounted: 0,
                budget: 98_900_000_000,
                available: 412_000_000_000,
                keep_free: 24_700_000_000,
            },
            keep: CheckoutsKeep::Within,
        },
        Finding::FirstRun {
            first: Duration::from_millis(337),
            then: Duration::from_millis(4),
            slows_a_suite: true,
        },
    ]
}

#[test]
fn what_the_machine_answers_for_holds_in_fewer_words_and_leaves_the_rest_to_doctor() {
    let found = the_machine();
    let panel = WhatItTakes {
        past: Some(&[]),
        machine: Some(&found),
    };
    assert_eq!(
        rows(&panel, &unicode()),
        [
            "what it usually takes",
            "0 past runs",
            "claude-code  2.1.0",
            "codex  0.157.0",
            "git  commits as Marta Ruiz",
            "forge  github eulke/yunta: `YUNTA_DESIGNED_FORGE_TOKEN`, the variable its token is \
             in, is not set, and `yunta/fragua` opens a pull request through it",
            "checkouts  builds 0 B of a 98.9 GB budget",
        ]
    );
}

/// An adapter that does not answer stands in the way of a run of a workflow it serves, and of
/// no other: a role only another workflow takes a session on names it.
#[test]
fn an_unhealthy_adapter_stands_in_the_way_of_the_workflows_it_serves_alone() {
    let found = [Finding::Adapter {
        adapter: AdapterId::from_static("codex"),
        health: AdapterHealth::Unhealthy {
            said: "codex: command not found".to_string(),
        },
        serving: vec!["yunta/fragua".to_string()],
    }];
    let panel = WhatItTakes {
        past: None,
        machine: Some(&found),
    };
    assert!(
        panel.in_the_way("greet").is_empty(),
        "a workflow codex serves no session of"
    );
    assert_eq!(
        panel.in_the_way("yunta/fragua").len(),
        1,
        "one codex serves"
    );
}

/// A workflow whose mode asks only what it cannot decide, with a node that always asks before
/// it decides: the hint names where it always asks, and says it asks only what it cannot decide
/// everywhere else.
#[test]
fn a_mixed_hint_names_where_the_workflow_always_asks() {
    let declared = r#"
name: mixed
modes:
  quick: { include: [grill, review], ask: required }
nodes:
  - id: grill
    kind: prompt
    runner: planner
    prompt: "Ask what only a person can decide."
    artifacts:
      produces: [questions]
  - id: review
    kind: prompt
    runner: planner
    prompt: "Ask what the work leaves open."
    ask: always
    depends_on: [grill]
    artifacts:
      produces: [questions]
"#;
    let workflow =
        yunta_core::workflow::read::read(declared, Path::new("mixed.yaml")).expect("a workflow");
    let quick = mode("quick");
    let new_run = NewRun {
        workflow: &workflow,
        mode: &quick,
    };
    assert_eq!(
        new_run.hint().as_deref(),
        Some(
            "mixed takes no request; mixed asks before it decides for you at review, and \
             elsewhere only what it cannot decide"
        )
    );
}
