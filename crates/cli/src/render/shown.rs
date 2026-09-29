//! A document an escalation shows, as the person deciding reads it.
//!
//! What they decide on is what the run holds, so the words come from the
//! document itself and the path under them is where its file sits. A
//! plan is read the way it is reviewed: what it changes and why, the
//! shapes it creates, then task by task what each does, what it touches
//! and what proves it done. A diagram has no room on a terminal, so it
//! is named here and drawn in the whole plan, one open away.

use yunta_core::events::ArtifactId;
use yunta_core::{Task, TasksFile};
use yunta_engine::{ShownContent, ShownDocument};

/// The lines `document` takes: a heading, what it says, and the file.
pub(crate) fn shown(document: &ShownDocument) -> Vec<String> {
    let of = document
        .shown
        .producer
        .as_ref()
        .map(|node| format!(" of `{node}`"))
        .unwrap_or_default();
    let (mut lines, whole) = match &document.content {
        ShownContent::Tasks(file) => (plan(file, &of), "the whole plan"),
        ShownContent::Text(text) => (
            self::text(text, &document.shown.artifact, &of),
            "the whole document",
        ),
    };
    lines.push(format!("{whole}: {}", document.path.display()));
    lines
}

fn plan(file: &TasksFile, of: &str) -> Vec<String> {
    let mut lines = vec![format!(
        "the plan{of} — {}",
        yunta_core::text::counted(file.tasks.len(), "task")
    )];
    if let Some(summary) = said(&file.summary) {
        lines.push(format!("  {summary}"));
    }
    if let Some(description) = said(&file.description) {
        lines.push(String::new());
        lines.extend(markdown(description, "  "));
    }
    if let Some(design) = said(&file.design) {
        lines.push(String::new());
        lines.push("  design".to_string());
        lines.extend(markdown(design, "    "));
    }
    for (heading, items) in [("risks", &file.risks), ("out of scope", &file.out_of_scope)] {
        if !items.is_empty() {
            lines.push(String::new());
            lines.push(format!("  {heading}"));
            lines.extend(items.iter().map(|item| format!("    - {item}")));
        }
    }
    lines.push(String::new());
    for task in &file.tasks {
        lines.extend(task_lines(task));
    }
    lines
}

/// One task: its id and title, then what it does, what it may touch,
/// what proves it done and what it waits for, each under it.
fn task_lines(task: &Task) -> Vec<String> {
    let under = " ".repeat(task.id.as_str().chars().count() + 4);
    let mut lines = vec![format!("  {}  {}", task.id, task.title)];
    if let Some(description) = said(&task.description) {
        lines.extend(markdown(description, &under));
    }
    let scope: Vec<&str> = task.scope.iter().map(|glob| glob.as_str()).collect();
    lines.push(format!("{under}scope: {}", scope.join(", ")));
    for criterion in &task.criteria {
        let lead = match criterion.is_guard() {
            true => "keeps passing",
            false => "done when",
        };
        lines.push(match said(&criterion.proves) {
            Some(proves) => format!("{under}{lead}: {proves} — `{}`", criterion.cmd),
            None => format!("{under}{lead}: `{}`", criterion.cmd),
        });
    }
    if !task.depends_on.is_empty() {
        let after: Vec<&str> = task.depends_on.iter().map(|id| id.as_str()).collect();
        lines.push(format!("{under}after: {}", after.join(", ")));
    }
    lines
}

/// Markdown as a terminal shows it: every line under `indent`, code
/// blocks as written, and each `mermaid` block named rather than drawn.
fn markdown(text: &str, indent: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_diagram = false;
    for line in text.lines() {
        let fence = line.trim_start();
        if in_diagram {
            in_diagram = !fence.starts_with("```");
            continue;
        }
        if fence.starts_with("```mermaid") {
            in_diagram = true;
            lines.push(format!("{indent}(diagram: in the whole plan)"));
            continue;
        }
        lines.push(match line.is_empty() {
            true => String::new(),
            false => format!("{indent}{line}"),
        });
    }
    lines
}

/// Text that says something, trimmed.
fn said(text: &Option<String>) -> Option<&str> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// Any other document, whole: what a person approves is what they read.
fn text(text: &str, artifact: &ArtifactId, of: &str) -> Vec<String> {
    let mut lines = vec![match artifact {
        ArtifactId::Interpreted { kind } => format!("the {kind} document{of}"),
        ArtifactId::Opaque { name } => format!("{name}{of}"),
    }];
    lines.extend(text.lines().map(|line| match line.is_empty() {
        true => String::new(),
        false => format!("  {line}"),
    }));
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
            path: "artifacts/plan/tasks.yaml".into(),
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
        let drawn = shown(&tasks(
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
        ));
        assert_eq!(
            drawn,
            vec![
                "the plan of `plan` — 2 tasks",
                "",
                "  T001  Write the greeting",
                "        scope: hello.txt",
                "        done when: `test -f hello.txt`",
                "        keeps passing: `true`",
                "  T002  Say it twice",
                "        scope: hello.txt, README.md",
                "        done when: `grep -c hello hello.txt`",
                "        after: T001",
                "the whole plan: artifacts/plan/tasks.yaml",
            ],
            "the notes stay in the file"
        );
    }

    #[test]
    fn a_plan_says_what_it_changes_and_names_its_diagrams_rather_than_drawing_them() {
        let drawn = shown(&tasks(
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
        ));
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
                "  ```rust",
                "  greet(\"es\")",
                "  ```",
                "",
                "  design",
                "    ```rust",
                "    pub fn greet(lang: &str) -> String;",
                "    ```",
                "",
                "  risks",
                "    - Snapshot tests change",
                "",
                "  T001  Add the table",
                "        Every language gets its greeting.",
                "        scope: src/i18n.rs",
                "        done when: each language has its greeting — `cargo test i18n`",
                "the whole plan: artifacts/plan/tasks.yaml",
            ]
        );
    }

    #[test]
    fn a_text_document_is_shown_whole_under_its_name() {
        let drawn = shown(&document(
            ShownContent::Text("# Brief\n\nSay hello.\n".to_string()),
            ArtifactId::Opaque {
                name: "brief.md".to_string(),
            },
        ));
        assert_eq!(
            drawn,
            vec![
                "brief.md of `plan`",
                "  # Brief",
                "",
                "  Say hello.",
                "the whole document: artifacts/plan/tasks.yaml",
            ]
        );
    }
}
