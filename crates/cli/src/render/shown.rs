//! A document an escalation shows, as the person deciding reads it.
//!
//! What they decide on is what the run holds, so the words come from the
//! document itself and the path under them is where its file sits.
//! A tasks document is read the way a plan is reviewed — task by task,
//! with what each may touch and what proves it done; its notes stay in
//! the file, one open away.

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
    let mut lines = match &document.content {
        ShownContent::Tasks(file) => tasks(file, &of),
        ShownContent::Text(text) => self::text(text, &document.shown.artifact, &of),
    };
    lines.push(format!("the whole document: {}", document.path.display()));
    lines
}

fn tasks(file: &TasksFile, of: &str) -> Vec<String> {
    let mut lines = vec![format!(
        "the tasks document{of} — {}",
        yunta_core::text::counted(file.tasks.len(), "task")
    )];
    for task in &file.tasks {
        lines.extend(task_lines(task));
    }
    lines
}

/// One task: its id and title, then what it may touch, what proves it
/// done and what it waits for, each on a line of its own under it.
fn task_lines(task: &Task) -> Vec<String> {
    let under = " ".repeat(task.id.as_str().chars().count() + 4);
    let mut lines = vec![format!("  {}  {}", task.id, task.title)];
    let scope: Vec<&str> = task.scope.iter().map(|glob| glob.as_str()).collect();
    lines.push(format!("{under}scope: {}", scope.join(", ")));
    let criteria: Vec<String> = task
        .criteria
        .iter()
        .map(|criterion| match criterion.is_guard() {
            true => format!("guard `{}`", criterion.cmd),
            false => format!("`{}`", criterion.cmd),
        })
        .collect();
    lines.push(format!("{under}criteria: {}", criteria.join("; ")));
    if !task.depends_on.is_empty() {
        let after: Vec<&str> = task.depends_on.iter().map(|id| id.as_str()).collect();
        lines.push(format!("{under}after: {}", after.join(", ")));
    }
    lines
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

    #[test]
    fn a_plan_is_shown_task_by_task_with_its_scope_and_what_proves_it_done() {
        let file: TasksFile = serde_norway::from_str(
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
        )
        .unwrap();
        let drawn = shown(&document(
            ShownContent::Tasks(file),
            ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks,
            },
        ));
        assert_eq!(
            drawn,
            vec![
                "the tasks document of `plan` — 2 tasks",
                "  T001  Write the greeting",
                "        scope: hello.txt",
                "        criteria: `test -f hello.txt`; guard `true`",
                "  T002  Say it twice",
                "        scope: hello.txt, README.md",
                "        criteria: `grep -c hello hello.txt`",
                "        after: T001",
                "the whole document: artifacts/plan/tasks.yaml",
            ],
            "the notes stay in the file"
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
