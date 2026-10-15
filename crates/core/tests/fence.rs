//! The fence: which of a session's writes its scope admits, judged by one
//! pure function against the session's confinement.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_core::fence::{Advice, Coverage, Fence, Fenced, Verdict, REFUSAL_MARKER};
use yunta_core::port::PermissionProfile;
use yunta_core::protected::{Protected, ProtectedError, Protection};
use yunta_core::{Confinement, ConfinementInput, EditSandbox, ScopeGlob, ToolNaming};

fn worktree() -> PathBuf {
    PathBuf::from("/work/run-1/task-1")
}

/// A fence whose scope admits `allowed`, advising a finding.
fn scoped(allowed: &[&str]) -> Fence {
    Fence {
        allowed: Some(allowed.iter().map(|g| ScopeGlob::from(*g)).collect()),
        advice: Advice::ReportFinding,
    }
}

/// The confinement of a session of `profile` over the worktree.
fn confined(
    profile: PermissionProfile,
    shared_dirs: &[PathBuf],
    artifact_dir: Option<&Path>,
    protected: Protected,
) -> Confinement {
    Confinement::of(ConfinementInput {
        profile,
        edit: EditSandbox::default(),
        scoped: true,
        network: true,
        checkout: &worktree(),
        artifact_dir,
        shared_dirs,
        protected: Arc::new(protected),
        git_register: None,
    })
    .expect("a session over the worktree is buildable")
}

/// The confinement of an edit session with nothing shared beside the worktree.
fn edit_session() -> Confinement {
    confined(PermissionProfile::Edit, &[], None, Protected::default())
}

#[test]
fn a_write_outside_every_glob_is_refused_naming_the_target() {
    let Verdict::Refused(refusal) =
        scoped(&["src/**"]).judge(&worktree(), Path::new("docs/readme.md"), &edit_session())
    else {
        panic!("a write outside every glob is refused");
    };
    assert_eq!(refusal.target, worktree().join("docs/readme.md"));
}

#[test]
fn a_write_inside_an_allowed_glob_is_allowed() {
    assert_eq!(
        scoped(&["src/**"]).judge(&worktree(), Path::new("src/deep/lib.rs"), &edit_session()),
        Verdict::Allowed
    );
}

#[test]
fn a_write_under_a_root_is_allowed_whatever_the_globs_say() {
    let shared = [PathBuf::from("/work/run-1/artifacts")];
    let confinement = confined(PermissionProfile::Edit, &shared, None, Protected::default());
    assert_eq!(
        scoped(&[]).judge(
            &worktree(),
            Path::new("/work/run-1/artifacts/tasks.yaml"),
            &confinement
        ),
        Verdict::Allowed
    );
}

#[test]
fn a_write_into_dot_git_is_refused() {
    let fence = Fence::everything(Advice::ReportFinding);
    let confinement = edit_session();
    for target in [".git", ".git/config", ".git/refs/heads/main"] {
        assert!(
            matches!(
                fence.judge(&worktree(), Path::new(target), &confinement),
                Verdict::Refused(_)
            ),
            "the run's own register is never a task's work: {target}"
        );
    }
}

#[test]
fn a_relative_target_is_judged_against_the_worktree() {
    let fence = scoped(&["src/**"]);
    let confinement = edit_session();
    assert_eq!(
        fence.judge(&worktree(), Path::new("src/lib.rs"), &confinement),
        fence.judge(&worktree(), &worktree().join("src/lib.rs"), &confinement)
    );
}

#[test]
fn a_dot_dot_escape_is_refused_without_touching_disk() {
    let fence = Fence::everything(Advice::ReportFinding);
    let Verdict::Refused(refusal) =
        fence.judge(&worktree(), Path::new("../../etc/passwd"), &edit_session())
    else {
        panic!("a path that climbs out of the worktree is refused");
    };
    assert_eq!(refusal.target, PathBuf::from("/work/etc/passwd"));
}

#[test]
fn an_empty_allowed_set_refuses_every_worktree_write_and_keeps_the_roots() {
    let artifacts = PathBuf::from("/work/run-1/artifacts");
    let confinement = confined(
        PermissionProfile::ReadOnly,
        &[],
        Some(artifacts.as_path()),
        Protected::default(),
    );
    let fence = Fence::for_session(
        PermissionProfile::ReadOnly,
        None,
        &[],
        Advice::ReportFinding,
    );

    assert!(matches!(
        fence.judge(&worktree(), Path::new("src/lib.rs"), &confinement),
        Verdict::Refused(_)
    ));
    assert_eq!(
        fence.judge(
            &worktree(),
            Path::new("/work/run-1/artifacts/findings.yaml"),
            &confinement
        ),
        Verdict::Allowed
    );
}

#[test]
fn a_read_only_confinement_refuses_a_write_under_the_worktree_whatever_the_scope() {
    let confinement = confined(PermissionProfile::ReadOnly, &[], None, Protected::default());
    for fence in [Fence::everything(Advice::ReportFinding), scoped(&["**"])] {
        let Verdict::Refused(refusal) =
            fence.judge(&worktree(), Path::new("src/lib.rs"), &confinement)
        else {
            panic!("a read-only session writes nothing under the worktree");
        };
        assert_eq!(refusal.allowed, Some(Vec::new()));
        let text = refusal.message(ToolNaming::Bare);
        assert!(
            text.contains("Allowed under /work/run-1/task-1: none;"),
            "{text}"
        );
    }
}

#[test]
fn a_granted_expansion_is_inside_the_fence() {
    let scope = [ScopeGlob::from("src/**")];
    let granted = [ScopeGlob::from("docs/**")];
    let fence = Fence::for_session(
        PermissionProfile::Edit,
        Some(&scope),
        &granted,
        Advice::RequestExpansion,
    );

    assert_eq!(
        fence.judge(&worktree(), Path::new("docs/adr.md"), &edit_session()),
        Verdict::Allowed
    );
}

#[test]
fn a_refusal_names_the_tool_it_advises_the_way_the_session_s_cli_does() {
    let fence = Fence {
        advice: Advice::RequestExpansion,
        ..scoped(&["src/**"])
    };
    let Verdict::Refused(refusal) =
        fence.judge(&worktree(), Path::new("docs/readme.md"), &edit_session())
    else {
        panic!("outside every glob");
    };
    let text = refusal.message(ToolNaming::McpPrefixed);
    assert!(
        text.contains("mcp__yunta-run__yunta_request_scope_expansion"),
        "{text}"
    );
    assert!(text.starts_with(REFUSAL_MARKER), "{text}");
}

#[test]
fn a_refusal_message_names_its_target_and_says_its_advice() {
    for (advice, expected) in [
        (Advice::RequestExpansion, "yunta_request_scope_expansion"),
        (Advice::ReportFinding, "Report the need as a finding"),
    ] {
        let fence = Fence {
            advice,
            ..scoped(&["src/**"])
        };
        let Verdict::Refused(refusal) =
            fence.judge(&worktree(), Path::new("docs/readme.md"), &edit_session())
        else {
            panic!("outside every glob");
        };
        let text = refusal.message(ToolNaming::Bare);
        assert!(
            text.contains(&refusal.target.display().to_string()),
            "{text}"
        );
        assert!(
            text.contains(expected),
            "the advice reaches the model: {text}"
        );
    }
}

#[test]
fn coverage_is_the_weakest_channel() {
    let roots = || Fenced::Roots(vec![PathBuf::from("/work/run-1")]);
    assert_eq!(
        Coverage::of(Fenced::Exact, Some(Fenced::Exact)),
        Coverage::Exact
    );
    assert_eq!(Coverage::of(Fenced::Exact, None), Coverage::ToolsOnly);
    assert_eq!(Coverage::of(roots(), None), Coverage::ToolsOnly);
    assert_eq!(
        Coverage::of(Fenced::Exact, Some(roots())),
        Coverage::WidenedToRoots {
            roots: vec![PathBuf::from("/work/run-1")]
        }
    );
    assert_eq!(
        Coverage::of(roots(), Some(Fenced::Exact)),
        Coverage::WidenedToRoots {
            roots: vec![PathBuf::from("/work/run-1")]
        }
    );
    assert_eq!(
        Coverage::of(roots(), Some(roots())),
        Coverage::WidenedToRoots {
            roots: vec![PathBuf::from("/work/run-1")]
        }
    );
}

/// What the project denies to every run is refused whatever the scope
/// admits, and the refusal says no request widens it.
#[test]
fn a_denied_path_is_refused_where_the_scope_allows_it() -> Result<(), ProtectedError> {
    let protected = Protected::new(&[ScopeGlob::from(".github/**")], &[])?;
    let confinement = confined(PermissionProfile::Edit, &[], None, protected);
    let fence = scoped(&["**"]);

    let Verdict::Refused(refusal) =
        fence.judge(&worktree(), Path::new(".github/ci.yml"), &confinement)
    else {
        panic!("a denied path is refused");
    };
    assert_eq!(refusal.protection, Some(Protection::ProjectDeny));
    let said = refusal.message(ToolNaming::Bare);
    assert!(said.contains("no request widens"), "{said}");
    assert!(said.contains(".github/ci.yml"), "{said}");
    assert_eq!(
        fence.judge(&worktree(), Path::new("src/lib.rs"), &confinement),
        Verdict::Allowed
    );
    Ok(())
}

#[test]
fn an_unscoped_fence_still_refuses_denied_paths() -> Result<(), ProtectedError> {
    let protected = Protected::new(&[ScopeGlob::from(".github/**")], &[])?;
    let confinement = confined(PermissionProfile::Edit, &[], None, protected);
    let fence = Fence::everything(Advice::ReportFinding);
    assert!(matches!(
        fence.judge(&worktree(), Path::new(".github/ci.yml"), &confinement),
        Verdict::Refused(_)
    ));
    assert_eq!(
        fence.judge(&worktree(), Path::new("README.md"), &confinement),
        Verdict::Allowed
    );
    Ok(())
}

/// A root inside the checkout does not admit what the project protects.
#[test]
fn a_protected_path_under_a_root_is_refused() -> Result<(), ProtectedError> {
    let claude = worktree().join(".claude");
    let protected = Protected::new(&[ScopeGlob::from(".claude/**")], &[])?;
    let confinement = confined(
        PermissionProfile::Edit,
        &[],
        Some(claude.as_path()),
        protected,
    );
    let verdict = Fence::everything(Advice::ReportFinding).judge(
        &worktree(),
        Path::new(".claude/settings.json"),
        &confinement,
    );
    let by_project = matches!(
        &verdict,
        Verdict::Refused(refusal) if refusal.protection == Some(Protection::ProjectDeny)
    );
    assert!(by_project, "{verdict:?}");
    Ok(())
}
