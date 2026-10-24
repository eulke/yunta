use super::*;

#[test]
fn the_register_is_its_first_name_whatever_dots_come_before_it() {
    assert!(is_register(Path::new(".git")));
    assert!(is_register(Path::new("./.git/config")));
    assert!(!is_register(Path::new(".gitignore")));
    assert!(!is_register(Path::new("src/.git")));
}

#[test]
fn the_strictest_reason_that_reaches_a_glob_is_the_one_nobody_may_grant() {
    let set = Protected::new(&[ScopeGlob::from(".github/**")], &[]).unwrap();
    assert_eq!(
        set.reached_by(&ScopeGlob::from("**")),
        Some(Protection::ProjectDeny)
    );
    assert_eq!(
        set.reached_by(&ScopeGlob::from("docs/**")),
        Some(Protection::AgentInstructions)
    );
}

#[test]
fn a_lifted_instruction_file_and_a_staged_path_are_no_trespass() {
    let set = Protected::new(&[], &[ScopeGlob::from("tests/approved/**")]).unwrap();
    let changed = [
        PathBuf::from("AGENTS.md"),
        PathBuf::from("tests/approved/a.rs"),
    ];
    assert_eq!(set.trespasses(&changed, &[], Closing::Every).len(), 2);
    let lifted = set.lifting(&[ScopeGlob::from("AGENTS.md")]);
    assert_eq!(
        lifted.trespasses(&changed, &[], Closing::Every),
        vec![Trespass {
            path: "tests/approved/a.rs".into(),
            reason: Protection::ApprovedTest,
        }]
    );
    let staged = [PathBuf::from("AGENTS.md")];
    assert_eq!(set.trespasses(&changed, &staged, Closing::Every).len(), 1);
}

#[test]
fn a_loop_close_leaves_the_approved_tests_to_their_task() {
    let set = Protected::new(&[], &[ScopeGlob::from("tests/approved/**")]).unwrap();
    let changed = [
        PathBuf::from("AGENTS.md"),
        PathBuf::from("tests/approved/a.rs"),
    ];
    assert_eq!(
        set.trespasses(&changed, &[], Closing::LoopClose),
        vec![Trespass {
            path: "AGENTS.md".into(),
            reason: Protection::AgentInstructions,
        }]
    );
}

#[test]
fn pathspecs_name_every_protection_but_the_register() {
    let set = Protected::new(&[ScopeGlob::from(".github/**")], &[]).unwrap();
    assert_eq!(
        set.pathspecs(),
        Pathspecs {
            rooted: [".github/**", ".claude/**", ".codex/**", ".agents/skills/**"]
                .map(|glob| format!(":(glob){glob}"))
                .to_vec(),
            loose: [".mcp.json", "**/AGENTS.md", "**/CLAUDE.md"]
                .map(|glob| format!(":(glob){glob}"))
                .to_vec(),
            points: [".claude", ".codex", ".agents/skills", ".agents"]
                .map(PathBuf::from)
                .to_vec(),
        }
    );
}

/// Of `.agents/`, only the skills root Codex scans is configuration: the
/// rest is the project's, an instruction file there is still one, and the
/// directories on the way to the skills are held as themselves.
#[test]
fn only_the_skills_root_of_agents_is_configuration() {
    let set = Protected::new(&[], &[]).unwrap();
    let reason = |path: &str| set.reason_of(Path::new(path));
    assert_eq!(
        reason(".agents/skills/x/SKILL.md"),
        Some(Protection::AgentConfig)
    );
    assert_eq!(reason(".agents/notes.md"), None);
    assert_eq!(
        reason(".agents/AGENTS.md"),
        Some(Protection::AgentInstructions)
    );
    assert_eq!(reason(".claude/CLAUDE.md"), Some(Protection::AgentConfig));
    assert_eq!(reason(".agents"), Some(Protection::AgentConfig));
    assert_eq!(reason(".agents/skills"), Some(Protection::AgentConfig));
}

#[test]
fn a_lift_removes_the_instruction_reason_and_no_other() {
    let set = Protected::new(&[ScopeGlob::from("**/*.md")], &[]).unwrap();
    let lifted = set.lifting(&[ScopeGlob::from("AGENTS.md")]);
    assert_eq!(
        lifted.reason_of(Path::new("AGENTS.md")),
        Some(Protection::ProjectDeny)
    );
}

#[test]
fn the_set_is_stored_as_its_entries_and_grants_and_built_again_on_read() {
    let set = Protected::new(&[ScopeGlob::from(".github/**")], &[]).unwrap();
    let text = serde_json::to_string(&set).unwrap();
    let read: Protected = serde_json::from_str(&text).unwrap();
    assert_eq!(read, set);
    assert_eq!(
        read.reason_of(Path::new(".github/ci.yml")),
        Some(Protection::ProjectDeny)
    );
    let lifted = set.lifting(&[ScopeGlob::from("AGENTS.md")]);
    assert_ne!(lifted, set);
    let text = serde_json::to_string(&lifted).unwrap();
    let read: Protected = serde_json::from_str(&text).unwrap();
    assert_eq!(read, lifted);
    assert_eq!(read.reason_of(Path::new("AGENTS.md")), None);
}
