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
//! drawn there.

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
        ShownContent::Tasks(file) => (super::plan::plan(file, &of, width), "the whole plan"),
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
    use yunta_core::events::Shown;
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
        document(
            ShownContent::Tasks(serde_norway::from_str(yaml).unwrap()),
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
        let drawn = shown(
            &document(
                ShownContent::Tasks(example),
                ArtifactId::Interpreted {
                    kind: ArtifactKind::Tasks,
                },
            ),
            78,
        );
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
