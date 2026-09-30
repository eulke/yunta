//! The fence: what a session may write, judged by one pure function.

use std::path::{Path, PathBuf};

use yunta_core::fence::{
    refused_target, Advice, Coverage, Fence, FenceHook, Fenced, Verdict, SUBCOMMAND,
};
use yunta_core::ScopeGlob;

fn worktree() -> PathBuf {
    PathBuf::from("/work/run-1/task-1")
}

fn fence(allowed: &[&str], roots: &[&str]) -> Fence {
    Fence {
        allowed: Some(allowed.iter().map(|g| ScopeGlob::from(*g)).collect()),
        roots: roots.iter().map(PathBuf::from).collect(),
        advice: Advice::ReportFinding,
        denied: Vec::new(),
    }
}

#[test]
fn a_write_outside_every_glob_is_refused_naming_the_target() {
    let fence = fence(&["src/**"], &[]);

    let Verdict::Refused(refusal) = fence.judge(&worktree(), Path::new("docs/readme.md")) else {
        panic!("a write outside every glob is refused");
    };
    assert_eq!(refusal.target, worktree().join("docs/readme.md"));
}

#[test]
fn a_write_inside_an_allowed_glob_is_allowed() {
    let fence = fence(&["src/**"], &[]);
    assert_eq!(
        fence.judge(&worktree(), Path::new("src/deep/lib.rs")),
        Verdict::Allowed
    );
}

#[test]
fn a_write_under_a_root_is_allowed_whatever_the_globs_say() {
    let fence = fence(&[], &["/work/run-1/artifacts"]);
    assert_eq!(
        fence.judge(&worktree(), Path::new("/work/run-1/artifacts/tasks.yaml")),
        Verdict::Allowed
    );
}

#[test]
fn a_write_into_dot_git_is_refused() {
    let fence = Fence::everything(Vec::new(), Advice::ReportFinding);
    for target in [".git", ".git/config", ".git/refs/heads/main"] {
        assert!(
            matches!(
                fence.judge(&worktree(), Path::new(target)),
                Verdict::Refused(_)
            ),
            "the run's own register is never a task's work: {target}"
        );
    }
}

#[test]
fn a_relative_target_is_judged_against_the_worktree() {
    let fence = fence(&["src/**"], &[]);
    assert_eq!(
        fence.judge(&worktree(), Path::new("src/lib.rs")),
        fence.judge(&worktree(), &worktree().join("src/lib.rs"))
    );
}

#[test]
fn a_dot_dot_escape_is_refused_without_touching_disk() {
    let fence = Fence::everything(Vec::new(), Advice::ReportFinding);
    let Verdict::Refused(refusal) = fence.judge(&worktree(), Path::new("../../etc/passwd")) else {
        panic!("a path that climbs out of the worktree is refused");
    };
    assert_eq!(refusal.target, PathBuf::from("/work/etc/passwd"));
}

#[test]
fn an_empty_allowed_set_refuses_every_worktree_write_and_keeps_the_roots() {
    let fence = Fence::read_only(
        vec![PathBuf::from("/work/run-1/artifacts")],
        Advice::ReportFinding,
    );

    assert!(matches!(
        fence.judge(&worktree(), Path::new("src/lib.rs")),
        Verdict::Refused(_)
    ));
    assert_eq!(
        fence.judge(
            &worktree(),
            Path::new("/work/run-1/artifacts/findings.yaml")
        ),
        Verdict::Allowed
    );
}

#[test]
fn a_granted_expansion_is_inside_the_fence() {
    let scope = [ScopeGlob::from("src/**")];
    let granted = [ScopeGlob::from("docs/**")];
    let fence = Fence::for_session(
        yunta_core::port::PermissionProfile::Edit,
        Some(&scope),
        &granted,
        None,
        Advice::RequestExpansion,
    );

    assert_eq!(
        fence.judge(&worktree(), Path::new("docs/adr.md")),
        Verdict::Allowed
    );
}

#[test]
fn a_refusal_message_round_trips_its_target_and_says_its_advice() {
    for (advice, expected) in [
        (Advice::RequestExpansion, "yunta_request_scope_expansion"),
        (Advice::ReportFinding, "Report the need as a finding"),
    ] {
        let fence = Fence {
            allowed: Some(vec![ScopeGlob::from("src/**")]),
            roots: Vec::new(),
            advice,
            denied: Vec::new(),
        };
        let Verdict::Refused(refusal) = fence.judge(&worktree(), Path::new("docs/readme.md"))
        else {
            panic!("outside every glob");
        };
        let text = refusal.to_string();
        assert_eq!(refused_target(&text), Some(refusal.target.clone()));
        assert!(
            text.contains(expected),
            "the advice reaches the model: {text}"
        );
    }
}

#[test]
fn text_without_the_marker_is_not_a_refusal() {
    assert_eq!(refused_target("Edit succeeded on src/lib.rs"), None);
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

#[test]
fn a_fence_round_trips_through_the_environment() {
    let fence = fence(&["src/**"], &["/work/run-1/artifacts"]);
    let (var, value) = fence.to_env(&worktree());
    assert_eq!(var, yunta_core::fence::ENV_VAR);

    let (read, worktree_read) = Fence::from_env(&value).unwrap();
    assert_eq!(read, fence);
    assert_eq!(worktree_read, worktree());
}

#[test]
fn a_fence_hook_names_the_subcommand_and_the_adapter() {
    let hook = FenceHook::new(PathBuf::from("/usr/local/bin/yunta"));
    assert_eq!(
        hook.command(&yunta_core::AdapterId::from("claude-code")),
        vec![
            "/usr/local/bin/yunta".to_string(),
            SUBCOMMAND.to_string(),
            "claude-code".to_string()
        ]
    );
}

/// What the project denies to every run is refused whatever the scope
/// admits, and the refusal says no request widens it.
#[test]
fn a_denied_path_is_refused_where_the_scope_allows_it() {
    let fence = fence(&["**"], &[]).denying(&[ScopeGlob::from(".github/**")]);

    let Verdict::Refused(refusal) = fence.judge(&worktree(), Path::new(".github/ci.yml")) else {
        panic!("a denied path is refused");
    };
    assert!(refusal.denied);
    let said = refusal.to_string();
    assert!(said.contains("no request widens"), "{said}");
    assert_eq!(
        refused_target(&said),
        Some(worktree().join(".github/ci.yml"))
    );
    assert_eq!(
        fence.judge(&worktree(), Path::new("src/lib.rs")),
        Verdict::Allowed
    );
}

#[test]
fn an_unscoped_fence_still_refuses_denied_paths() {
    let fence = Fence::everything(Vec::new(), Advice::ReportFinding)
        .denying(&[ScopeGlob::from(".github/**")]);
    assert!(matches!(
        fence.judge(&worktree(), Path::new(".github/ci.yml")),
        Verdict::Refused(_)
    ));
    assert_eq!(
        fence.judge(&worktree(), Path::new("README.md")),
        Verdict::Allowed
    );
}

#[test]
fn denied_globs_survive_the_env() {
    let fence = Fence::everything(Vec::new(), Advice::ReportFinding)
        .denying(&[ScopeGlob::from(".github/**")]);
    let (_, value) = fence.to_env(&worktree());
    let (read, _) = Fence::from_env(&value).expect("the fence reads back");
    assert_eq!(read, fence);
}
