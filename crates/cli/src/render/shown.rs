//! A document an escalation shows, as the person deciding reads it.
//!
//! What they decide on is what the run holds, so the words come from the
//! document itself, cut to the width they read at, and the path under
//! them is where its file sits, in full. A plan is read the way it is
//! reviewed: what it changes and why, the shapes it creates, what it
//! risks and leaves out, the order its tasks run in, then task by task
//! what each does, what it touches and what proves it done — in words;
//! the commands that prove it stay in the whole plan, one open away. A
//! diagram has no room on a terminal either, so it is named here and
//! drawn there. A spec is read task by task, with its tests' files
//! whole; what a review found, the most severe first; the run's findings
//! the same way, each with the node that found it and how others
//! answered it.

use yunta_core::events::ArtifactId;
use yunta_engine::{ShownContent, ShownDocument};

use crate::render::markdown::markdown;
use crate::render::INDENT;

/// The lines `document` takes at `width` cells: a heading, what it says,
/// and where its file is.
pub(crate) fn shown(document: &ShownDocument, width: usize) -> Vec<String> {
    let of = document
        .shown
        .producer
        .as_ref()
        .map(|node| format!(" of `{node}`"))
        .unwrap_or_default();
    let (mut lines, whole) = match &document.content {
        ShownContent::Tasks { plan, departed } => (
            super::plan::plan(plan, departed, &of, width),
            "the whole plan",
        ),
        ShownContent::Spec(file) => (super::spec::spec(file, &of, width), "the whole document"),
        ShownContent::Findings(file) => (
            super::findings::findings(file, &of, width),
            "the whole document",
        ),
        ShownContent::RunFindings(view) => (
            super::findings::run_findings(view, width),
            "every finding, whole",
        ),
        ShownContent::Text(text) => (
            self::text(text, &document.shown.artifact, &of, width),
            "the whole document",
        ),
    };
    lines.push(String::new());
    lines.push(format!("{whole}: {}", document.path.display()));
    lines
}

/// Any other document, whole: what a person approves is what they read.
fn text(text: &str, artifact: &ArtifactId, of: &str, width: usize) -> Vec<String> {
    let mut lines = vec![match artifact {
        ArtifactId::Interpreted { kind } => format!("the {kind} document{of}"),
        ArtifactId::Opaque { name } => format!("{name}{of}"),
    }];
    lines.extend(markdown(text, INDENT, width));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::cell_width;
    use yunta_core::events::{AcceptedDeparture, DepartsFrom, DeviationDeclaredPayload, Shown};
    use yunta_core::shape::Document;
    use yunta_core::{ArtifactKind, TasksFile};

    fn document(content: ShownContent, artifact: ArtifactId) -> ShownDocument {
        ShownDocument {
            shown: Shown {
                producer: Some("plan".into()),
                artifact,
                content_hash: yunta_core::sha256_hex(b"doc"),
            },
            path: "/runs/r/artifacts/plan/tasks.md".into(),
            content,
        }
    }

    fn tasks(yaml: &str) -> ShownDocument {
        plan(serde_norway::from_str(yaml).unwrap(), Vec::new())
    }

    fn plan(plan: TasksFile, departed: Vec<AcceptedDeparture>) -> ShownDocument {
        document(
            ShownContent::Tasks { plan, departed },
            ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks,
            },
        )
    }

    #[test]
    fn a_plan_with_nothing_for_a_person_is_shown_task_by_task() {
        let drawn = shown(
            &tasks(
                r#"
tasks:
  - id: T001
    title: Write the greeting
    scope: [hello.txt]
    criteria:
      - cmd: test -f hello.txt
      - { cmd: "true", type: guard }
    notes: the greeting lives in hello.txt
  - id: T002
    title: Say it twice
    scope: [hello.txt, README.md]
    criteria: [{ cmd: "grep -c hello hello.txt" }]
    depends_on: [T001]
"#,
            ),
            78,
        );
        assert_eq!(
            drawn,
            vec![
                "the plan of `plan` — 2 tasks in 2 steps",
                "",
                "  order",
                "    1  T001",
                "    2  T002",
                "",
                "  T001 — Write the greeting",
                "    touches       hello.txt",
                "    done when     `test -f hello.txt`",
                "    keeps passing `true`",
                "",
                "  T002 — Say it twice",
                "    touches       hello.txt, README.md",
                "    done when     `grep -c hello hello.txt`",
                "    after         T001",
                "",
                "the whole plan: /runs/r/artifacts/plan/tasks.md",
            ],
            "a criterion that says nothing of what it proves is named by its command; the \
             notes stay in the file"
        );
    }

    #[test]
    fn a_plan_says_what_it_changes_and_names_its_diagrams_rather_than_drawing_them() {
        let drawn = shown(
            &tasks(
                r#"
summary: Greet in the user's language
description: |
  Greetings come from a table.

  ```mermaid
  graph LR
    Table --> Greeter
  ```

  ```rust
  greet("es")
  ```
design: |
  ```rust
  pub fn greet(lang: &str) -> String;
  ```
risks: [Snapshot tests change]
tasks:
  - id: T001
    title: Add the table
    description: Every language gets its greeting.
    scope: [src/i18n.rs]
    criteria:
      - { cmd: "cargo test i18n", proves: "each language has its greeting" }
"#,
            ),
            78,
        );
        assert_eq!(
            drawn,
            vec![
                "the plan of `plan` — 1 task",
                "  Greet in the user's language",
                "",
                "  Greetings come from a table.",
                "",
                "  (diagram: in the whole plan)",
                "",
                "    greet(\"es\")",
                "",
                "  design",
                "      pub fn greet(lang: &str) -> String;",
                "",
                "  risks",
                "    - Snapshot tests change",
                "",
                "  T001 — Add the table",
                "    Every language gets its greeting.",
                "    touches       src/i18n.rs",
                "    done when     each language has its greeting",
                "",
                "the whole plan: /runs/r/artifacts/plan/tasks.md",
            ]
        );
    }

    #[test]
    fn every_line_of_a_plan_fits_the_width_it_is_read_at() {
        let long = "word ".repeat(90);
        let drawn = shown(
            &tasks(&format!(
                r#"
summary: {long}
description: {long}
design: |
  ```rust
  pub(crate) fn a_signature_far_wider_than_any_terminal_it_will_be_read_on(first: FirstArgument, second: SecondArgument) -> Result<(), Error>;
  ```
tasks:
  - id: color-command-output
    title: {long}
    description: {long}
    scope: [crates/cli/src/commands/list/runs.rs, crates/cli/src/commands/stats.rs, crates/cli/src/render/bars.rs, crates/cli/src/render/state.rs, crates/cli/tests/stats_cmd.rs]
    criteria:
      - {{ cmd: "grep -q 'fn tables_and_bars_use_standard_color_roles' crates/cli/src/commands/stats.rs && cargo test -p yunta --bin yunta -- tables_and_bars_use_standard_color_roles", proves: "{long}" }}
"#
            )),
            78,
        );
        let wide: Vec<&String> = drawn
            .iter()
            .filter(|line| !line.starts_with("the whole plan"))
            .filter(|line| cell_width(line) > 78)
            .collect();
        assert!(wide.is_empty(), "{wide:#?}");
        assert!(
            !drawn.iter().any(|line| line.contains("cargo test")),
            "a criterion that says what it proves is read by what it proves"
        );
        assert!(
            drawn
                .iter()
                .any(|line| line.ends_with("crates/cli/src/render/{bars.rs, state.rs}")),
            "a directory's globs stay on one line: {drawn:#?}"
        );
    }

    #[test]
    fn a_plan_puts_its_decisions_and_shapes_before_its_tasks_and_says_what_each_task_changes() {
        let example: TasksFile =
            yunta_core::shape::read(TasksFile::EXAMPLE.as_bytes(), "example").unwrap();
        let drawn = shown(&plan(example, Vec::new()), 78);
        let at = |text: &str| {
            drawn
                .iter()
                .position(|line| line.contains(text))
                .unwrap_or_else(|| panic!("`{text}` is not drawn: {drawn:#?}"))
        };
        assert!(at("  decisions") < at("  design") && at("  design") < at("add-dark-mode — "));
        at("- Which theme does a new user start with? — Light");
        at("because Nothing changes for anyone who never opens the setting");
        at("Theme — built by add-dark-mode, in src/theme/mod.rs");
        at("pub enum Theme { Light, Dark }");
        at("you will see  The settings screen has a dark mode switch");
        at("changes       - src/theme/mod.rs::Theme: the enum, and the setting that");
        at("uses          Theme");
    }

    #[test]
    fn a_departure_a_person_accepted_is_read_before_the_plan_it_departs_from() {
        let example: TasksFile =
            yunta_core::shape::read(TasksFile::EXAMPLE.as_bytes(), "example").unwrap();
        let departed = vec![AcceptedDeparture {
            declared: DeviationDeclaredPayload {
                task_id: "add-dark-mode".into(),
                from: DepartsFrom::Shape("Theme".to_string()),
                planned: "pub enum Theme { Light, Dark }".to_string(),
                instead: "pub enum Theme { Light, Dark, System }, since the platform \
                          already says which one the person prefers"
                    .to_string(),
                why: "a user who set their system to dark would open the app in light".to_string(),
            },
            said: Some("fine, keep System".to_string()),
        }];
        let drawn = shown(&plan(example, departed), 60);
        let at = |text: &str| {
            drawn
                .iter()
                .position(|line| line.contains(text))
                .unwrap_or_else(|| panic!("`{text}` is not drawn: {drawn:#?}"))
        };
        assert!(at("  accepted departures from this plan") < at("  decisions"));
        let card = at("- add-dark-mode departs from shape `Theme`");
        assert_eq!(
            drawn[card..=card + 8],
            [
                "    - add-dark-mode departs from shape `Theme`",
                "      the plan says pub enum Theme { Light, Dark }",
                "      built instead pub enum Theme { Light, Dark, System },",
                "                    since the platform already says which",
                "                    one the person prefers",
                "      because       a user who set their system to dark",
                "                    would open the app in light",
                "      accepted with fine, keep System",
                "",
            ]
        );
        assert!(
            drawn.iter().all(|line| cell_width(line) <= 60),
            "{drawn:#?}"
        );
    }

    /// A finding another node fixed and its proof settled, and one the
    /// engine reported about the run.
    const ANSWERED: &str = r#"
findings:
  - node: review
    id: halfway
    severity: blocking
    title: "The greeting stops halfway"
    location: "greeting.txt"
    detail: "It never says good night."
    answers:
      - { by: fix, answer: fixed, why: "it says good night now" }
    proof: { by: fix, cmd: "grep -q night greeting.txt", exit_code: 0 }
    settled: { by: proof, cmd: "grep -q night greeting.txt" }
  - id: cleanup
    severity: minor
    title: "A cleanup did not finish"
    location: "run:scratch"
    detail: "Left behind."
"#;

    #[test]
    fn a_review_shown_after_it_was_answered_reads_each_answer_under_its_finding() {
        let view: yunta_core::events::findings::RunFindings =
            serde_norway::from_str(ANSWERED).unwrap();
        let drawn = shown(
            &document(
                ShownContent::RunFindings(view),
                ArtifactId::Interpreted {
                    kind: ArtifactKind::Findings,
                },
            ),
            60,
        );
        assert_eq!(
            drawn,
            vec![
                "the run's findings — 1 blocking, 1 minor; 1 answered, 1 settled",
                "",
                "  blocking — The greeting stops halfway",
                "    halfway of `review`, at greeting.txt",
                "    It never says good night.",
                "    fixed by `fix` — it says good night now",
                "    settled — `grep -q night greeting.txt` exits 0",
                "",
                "  minor — A cleanup did not finish",
                "    cleanup, the run's own, at run:scratch",
                "    Left behind.",
                "",
                "every finding, whole: /runs/r/artifacts/plan/tasks.md",
            ]
        );
    }

    #[test]
    fn a_review_is_read_the_most_severe_finding_first() {
        let found: yunta_core::FindingsFile = serde_norway::from_str(
            r#"
findings:
  - id: stray-space
    severity: minor
    title: "A stray space"
    location: "greeting.txt:1"
    detail: "Between the words."
  - id: halfway
    severity: blocking
    title: "The greeting stops halfway"
    location: "greeting.txt"
    detail: "It says hello and never good night, which the plan's `Greeting` shape promises."
"#,
        )
        .unwrap();
        let drawn = shown(
            &document(
                ShownContent::Findings(found),
                ArtifactId::Interpreted {
                    kind: ArtifactKind::Findings,
                },
            ),
            50,
        );
        assert_eq!(
            drawn,
            vec![
                "the findings of `plan` — 1 blocking, 1 minor",
                "",
                "  blocking — The greeting stops halfway",
                "    halfway, at greeting.txt",
                "    It says hello and never good night, which the",
                "    plan's `Greeting` shape promises.",
                "",
                "  minor — A stray space",
                "    stray-space, at greeting.txt:1",
                "    Between the words.",
                "",
                "the whole document: /runs/r/artifacts/plan/tasks.md",
            ]
        );
    }

    #[test]
    fn a_spec_is_read_task_by_task_with_its_tests_files_whole() {
        let spec: yunta_core::SpecFile = serde_norway::from_str(
            r#"
specs:
  - task: greet
    files:
      - path: tests/greet.sh
        content: "test \"$(cat greeting.txt)\" = Hello\n"
    tests:
      - cmd: sh tests/greet.sh
        proves: the greeting says hello
"#,
        )
        .unwrap();
        let drawn = shown(
            &document(
                ShownContent::Spec(spec),
                ArtifactId::Interpreted {
                    kind: ArtifactKind::Spec,
                },
            ),
            60,
        );
        assert_eq!(
            drawn[..drawn.len() - 2],
            [
                "the spec of `plan` — 1 test for 1 task",
                "",
                "  greet",
                "    proves the greeting says hello",
                "",
                "    tests/greet.sh",
                "      test \"$(cat greeting.txt)\" = Hello",
            ]
        );
    }

    #[test]
    fn a_text_document_is_shown_whole_under_its_name() {
        let drawn = shown(
            &document(
                ShownContent::Text("# Brief\n\nSay hello.\n".to_string()),
                ArtifactId::Opaque {
                    name: "brief.md".to_string(),
                },
            ),
            78,
        );
        assert_eq!(
            drawn,
            vec![
                "brief.md of `plan`",
                "  # Brief",
                "",
                "  Say hello.",
                "",
                "the whole document: /runs/r/artifacts/plan/tasks.md",
            ]
        );
    }
}
