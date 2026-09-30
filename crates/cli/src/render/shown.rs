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
use yunta_core::{ScopeGlob, Task, TasksFile};
use yunta_engine::{PlanView, ShownContent, ShownDocument};

use crate::render::markdown::{hanging, markdown};
use crate::render::{cell_width, INDENT};

/// Where a section's body sits: one step under its heading, which is one
/// step under the document's own.
const BODY: &str = "    ";

/// The column a task's facts are labelled in, wide enough for the
/// longest label.
const LABEL: usize = "keeps passing".len() + 1;

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
        ShownContent::Tasks(file) => (plan(file, &of, width), "the whole plan"),
        ShownContent::Text(text) => (
            self::text(text, &document.shown.artifact, &of, width),
            "the whole document",
        ),
    };
    lines.push(String::new());
    lines.push(format!("{whole}: {}", document.path.display()));
    lines
}

fn plan(file: &TasksFile, of: &str, width: usize) -> Vec<String> {
    let view = PlanView::of(file);
    let steps = view.steps();
    let tasks = yunta_core::text::counted(file.tasks.len(), "task");
    let mut lines = vec![match steps.len() {
        0 | 1 => format!("the plan{of} — {tasks}"),
        n => format!("the plan{of} — {tasks} in {n} steps"),
    }];
    if let Some(summary) = said(&file.summary) {
        lines.extend(hanging(INDENT, "", summary, width));
    }
    if let Some(description) = said(&file.description) {
        lines.push(String::new());
        lines.extend(markdown(description, INDENT, width));
    }
    if let Some(design) = said(&file.design) {
        heading(&mut lines, "design");
        lines.extend(markdown(design, BODY, width));
    }
    for (name, items) in [("risks", &file.risks), ("out of scope", &file.out_of_scope)] {
        if !items.is_empty() {
            heading(&mut lines, name);
            for item in items {
                lines.extend(hanging(BODY, "- ", item, width));
            }
        }
    }
    if steps.len() > 1 {
        heading(&mut lines, "order");
        for (at, step) in steps.iter().enumerate() {
            let ids: Vec<&str> = step.iter().map(|task| task.id.as_str()).collect();
            lines.extend(hanging(
                BODY,
                &format!("{}  ", at + 1),
                &ids.join(" · "),
                width,
            ));
        }
    }
    for task in &file.tasks {
        lines.push(String::new());
        lines.extend(card(task, width));
    }
    lines
}

/// One task: its id and title, what it does, and beside each label what
/// it touches, what proves it done, what it keeps passing and what it
/// waits for.
fn card(task: &Task, width: usize) -> Vec<String> {
    let mut lines = hanging(INDENT, &format!("{} — ", task.id), &task.title, width);
    if let Some(description) = said(&task.description) {
        lines.extend(markdown(description, BODY, width));
    }
    // One line when the globs fit on it, and a directory's to a line
    // when they do not, so no group is cut in two.
    let groups = grouped(&task.scope);
    let together = groups.join(", ");
    let room = width.saturating_sub(cell_width(BODY) + LABEL);
    let touches = match cell_width(&together) <= room {
        true => vec![together],
        false => groups,
    };
    lines.extend(labelled("touches", &touches, Mark::None, width));
    let proves = |guard: bool| -> Vec<String> {
        task.criteria
            .iter()
            .filter(|criterion| criterion.is_guard() == guard)
            .map(|criterion| match said(&criterion.proves) {
                Some(proves) => proves.to_string(),
                None => format!("`{}`", criterion.cmd),
            })
            .collect()
    };
    lines.extend(labelled("done when", &proves(false), Mark::Several, width));
    lines.extend(labelled(
        "keeps passing",
        &proves(true),
        Mark::Several,
        width,
    ));
    if !task.depends_on.is_empty() {
        let after: Vec<&str> = task.depends_on.iter().map(|id| id.as_str()).collect();
        lines.extend(labelled("after", &[after.join(", ")], Mark::None, width));
    }
    lines
}

/// Whether the items beside a label are marked as one of several.
#[derive(Clone, Copy)]
enum Mark {
    /// When there are several: each is a claim of its own.
    Several,
    /// Never: the items are one list, broken across lines.
    None,
}

/// `items` under `label`: the label once, in its column, and each item
/// on its own line beside it.
fn labelled(label: &str, items: &[String], mark: Mark, width: usize) -> Vec<String> {
    let mark = match mark {
        Mark::Several if items.len() > 1 => "- ",
        _ => "",
    };
    let mut lines = Vec::new();
    for (at, item) in items.iter().enumerate() {
        let name = if at == 0 { label } else { "" };
        lines.extend(hanging(BODY, &format!("{name:<LABEL$}{mark}"), item, width));
    }
    lines
}

/// `scope` as a reader scans it: globs that share a directory under that
/// directory once, in the order the task names them.
fn grouped(scope: &[ScopeGlob]) -> Vec<String> {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for glob in scope {
        let glob = glob.as_str();
        let (dir, name) = glob.rsplit_once('/').unwrap_or(("", glob));
        match groups.iter_mut().find(|(seen, _)| *seen == dir) {
            Some((_, names)) => names.push(name),
            None => groups.push((dir, vec![name])),
        }
    }
    groups
        .into_iter()
        .map(|(dir, names)| match (dir, names.as_slice()) {
            ("", _) => names.join(", "),
            (_, [one]) => format!("{dir}/{one}"),
            _ => format!("{dir}/{{{}}}", names.join(", ")),
        })
        .collect()
}

/// A heading of the plan's, a line after what came before it.
fn heading(lines: &mut Vec<String>, name: &str) {
    lines.push(String::new());
    lines.push(format!("{INDENT}{name}"));
}

/// Text that says something, trimmed.
fn said(text: &Option<String>) -> Option<&str> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
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
    use yunta_core::events::Shown;
    use yunta_core::ArtifactKind;

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
    fn a_task_s_scope_is_grouped_by_the_directory_its_globs_share() {
        let scope: Vec<ScopeGlob> = [
            "crates/cli/src/render/color.rs",
            "crates/cli/src/render/mod.rs",
            "crates/cli/src/error.rs",
            "Cargo.toml",
        ]
        .into_iter()
        .map(ScopeGlob::from)
        .collect();
        assert_eq!(
            grouped(&scope),
            vec![
                "crates/cli/src/render/{color.rs, mod.rs}",
                "crates/cli/src/error.rs",
                "Cargo.toml",
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
