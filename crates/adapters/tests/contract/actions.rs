//! One act, said the way each CLI says it, reads as one action on one object: a
//! reader of a run cannot tell from it which CLI made the call, and is told
//! nothing the session typed.

use yunta_core::events::ToolAction::{
    self, Call, Edit, Fetch, List, Read, Run, Search, Submit, Write,
};
use yunta_core::port::AgentEvent;
use yunta_core::{ArtifactKind, RunTool};

use super::cases::{completed, fresh};
use super::subject::{Step, Subject};

/// One act, what it reads as, and what a reader is told of it.
type Case = (Step, ToolAction, Option<&'static str>);

/// A command a session ran.
fn shell(command: &str) -> Step {
    Step::ToolUse {
        name: "Bash".to_string(),
        target: command.to_string(),
    }
}

/// What a session does through a shell, which every CLI reports as its command.
fn commands() -> Vec<Case> {
    vec![
        (shell("cargo test --quiet"), Run, Some("cargo test")),
        (shell("git diff --stat"), Run, Some("git diff")),
        (
            shell("/bin/zsh -lc 'cd src && cargo build'"),
            Run,
            Some("cargo build"),
        ),
        (shell("FOO=1 make release"), Run, Some("make")),
        (
            shell("cargo frobnicate --token hunter2"),
            Run,
            Some("cargo"),
        ),
        (
            shell("psql postgres://admin:hunter2@db.internal/prod"),
            Run,
            None,
        ),
        (shell("sed -n 1,40p src/lib.rs"), Read, Some("src/lib.rs")),
        (shell("cat src/lib.rs | head -5"), Read, Some("src/lib.rs")),
        (shell("cat hunter2"), Read, None),
        (shell("cd src && cat lib.rs"), Read, None),
        (shell("sed -i s/a/b/ src/lib.rs"), Edit, Some("src/lib.rs")),
        (
            shell("echo hunter2 > src/lib.rs"),
            Write,
            Some("src/lib.rs"),
        ),
        (shell("find . -name hunter2 -delete"), Run, Some("find")),
        (shell("rg hunter2 src"), Search, None),
        (shell("ls src"), List, None),
        (
            shell("curl https://example.com/?token=hunter2"),
            Fetch,
            None,
        ),
    ]
}

/// What a session does through a tool, which a CLI may have one of its own for.
/// `inside` is the absolute path of a file the checkout holds.
fn tools(inside: String) -> Vec<Case> {
    let other = Step::OtherMcp {
        server: "tracker".to_string(),
        tool: "create_issue".to_string(),
    };
    vec![
        (Step::ReadFile(inside), Read, Some("src/lib.rs")),
        (Step::ReadFile("/etc/hosts".to_string()), Read, None),
        (
            Step::EditFile("src/lib.rs".to_string()),
            Edit,
            Some("src/lib.rs"),
        ),
        (Step::EditFile("../outside.rs".to_string()), Edit, None),
        (
            Step::WriteFile("src/new.rs".to_string()),
            Write,
            Some("src/new.rs"),
        ),
        (
            Step::RunToolCalled(RunTool::Submit(ArtifactKind::Tasks)),
            Submit,
            Some("tasks"),
        ),
        (
            Step::RunToolCalled(RunTool::CheckArtifact),
            Call,
            Some("yunta_check_artifact"),
        ),
        (other, Call, Some("tracker:create_issue")),
    ]
}

/// Every case, in a checkout holding `src/lib.rs`, reads as it says under `subject`.
pub async fn a_tool_call_reads_as_the_same_action_under_every_cli(subject: &dyn Subject) {
    let (dir, req) = fresh();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    let inside = dir.path().join("src/lib.rs").display().to_string();
    let table: Vec<Case> = commands().into_iter().chain(tools(inside)).collect();
    let steps = table.iter().map(|(step, ..)| step.clone()).collect();
    let played = subject.play(dir.path(), req, &completed(steps)).await;
    let said: Vec<(ToolAction, Option<String>)> = played
        .events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolUse { action, target, .. } => {
                Some((*action, target.display().map(str::to_string)))
            }
            _ => None,
        })
        .collect();
    let expected: Vec<(ToolAction, Option<String>)> = table
        .iter()
        .map(|(_, action, shows)| (*action, shows.map(str::to_string)))
        .collect();
    assert_eq!(said, expected);
    let carried = format!("{:?}", played.events);
    for fragment in ["hunter2", "db.internal", "example.com", "frobnicate"] {
        assert!(!carried.contains(fragment), "`{fragment}` in `{carried}`");
    }
}
