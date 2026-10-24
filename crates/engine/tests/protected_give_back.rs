//! What a checkout keeps when the pool takes it back: what git ignores stays
//! for the next unit, except what the project protects.

mod pool_world;

use pool_world::{write, World};
use yunta_core::protected::Protected;
use yunta_core::ScopeGlob;

/// A protected path that git ignores is cleaned out of a checkout the pool
/// takes back, so the next unit finds the checkout without it.
#[tokio::test]
async fn a_protected_ignored_file_does_not_survive_the_give_back() {
    let world = World::new();
    let protected = Protected::new(&[ScopeGlob::from("target/**")], &[]).unwrap();
    world.pool.protect(protected.pathspecs());
    let (first, held) = world.open("T001").await;
    write(&first.worktree.join("target/marker"), "built");
    world.give_back(&first, held).await;

    let (second, _held) = world.open("T002").await;

    assert_eq!(second.worktree, first.worktree);
    assert!(!second.worktree.join("target/marker").exists());
}

/// A glob that names no directory cleans the ignored files git reaches
/// without entering a directory the project ignores whole: a build output
/// keeps what lies in it, an instruction file outside it goes.
#[tokio::test]
async fn a_give_back_cleans_an_ignored_instruction_file_and_keeps_a_build_output() {
    let world = World::new();
    world
        .pool
        .protect(Protected::new(&[], &[]).unwrap().pathspecs());
    let (first, held) = world.open("T001").await;
    // `notes/` holds files git ignores one by one, so git enters it.
    let exclude = yunta_testkit::git_output(
        &first.worktree,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/exclude",
        ],
    );
    write(std::path::Path::new(exclude.trim()), "notes/*.md\n");
    write(
        &first.worktree.join("target/pkg/AGENTS.md"),
        "a dependency's own",
    );
    write(
        &first.worktree.join("notes/AGENTS.md"),
        "written by a session",
    );
    world.give_back(&first, held).await;

    let (second, _held) = world.open("T002").await;

    assert_eq!(second.worktree, first.worktree);
    assert!(second.worktree.join("target/pkg/AGENTS.md").exists());
    assert!(!second.worktree.join("notes/AGENTS.md").exists());
}

/// A link git ignores at the root of a tree a CLI loads does not survive the
/// give back: the pool looks at what stands there, as the close does.
#[cfg(unix)]
#[tokio::test]
async fn a_give_back_removes_an_ignored_link_at_a_config_root() {
    let world = World::new();
    world
        .pool
        .protect(Protected::new(&[], &[]).unwrap().pathspecs());
    let (first, held) = world.open("T001").await;
    let exclude = yunta_testkit::git_output(
        &first.worktree,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/exclude",
        ],
    );
    write(std::path::Path::new(exclude.trim()), "/.codex\n");
    std::os::unix::fs::symlink("tools", first.worktree.join(".codex")).unwrap();
    world.give_back(&first, held).await;

    let (second, _held) = world.open("T002").await;

    assert_eq!(second.worktree, first.worktree);
    assert!(std::fs::symlink_metadata(second.worktree.join(".codex")).is_err());
}

/// What git ignores under `.agents/` stays for the next unit, as any of the
/// project's files does, except under the skills root a CLI loads.
#[tokio::test]
async fn a_give_back_keeps_an_ignored_note_under_agents_and_cleans_an_ignored_skill() {
    let world = World::new();
    world
        .pool
        .protect(Protected::new(&[], &[]).unwrap().pathspecs());
    let (first, held) = world.open("T001").await;
    let exclude = yunta_testkit::git_output(
        &first.worktree,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/exclude",
        ],
    );
    write(std::path::Path::new(exclude.trim()), "/.agents/\n");
    write(&first.worktree.join(".agents/notes.md"), "the project's");
    write(&first.worktree.join(".agents/skills/x/SKILL.md"), "planted");
    world.give_back(&first, held).await;

    let (second, _held) = world.open("T002").await;

    assert_eq!(second.worktree, first.worktree);
    assert!(second.worktree.join(".agents/notes.md").exists());
    assert!(!second.worktree.join(".agents/skills/x/SKILL.md").exists());
}
