use super::*;
use crate::protected::Protection;

/// A commit's tree: its links, each with its text, and its files.
struct Tree {
    links: BTreeMap<PathBuf, String>,
    files: Vec<PathBuf>,
}

impl Tree {
    fn resolved(&self) -> Vec<LoadedLink> {
        resolved(&self.links, |path| {
            self.files.iter().any(|file| file == path)
        })
    }
}

fn tree(links: &[(&str, &str)], files: &[&str]) -> Tree {
    Tree {
        links: links
            .iter()
            .map(|(path, text)| (PathBuf::from(path), text.to_string()))
            .collect(),
        files: files.iter().map(PathBuf::from).collect(),
    }
}

fn fixed() -> Protected {
    Protected::new(&[], &[]).expect("the fixed set builds")
}

/// `set` grown by the links of `held` a CLI loads through.
fn grown(set: Protected, held: &Tree) -> Protected {
    let loaded = set.loaded(held.resolved()).expect("the links resolve");
    set.following(&loaded).expect("the set builds")
}

fn reason(set: &Protected, path: &str) -> Option<Protection> {
    set.reason_of(Path::new(path))
}

#[test]
fn a_link_resolves_from_its_directory_through_every_link_on_the_way() {
    let held = tree(
        &[
            (".claude/hooks", "../tools/hooks"),
            ("tools", "vendor/tools"),
            (".claude/skills", "../shared/../skills"),
        ],
        &["vendor/tools/hooks/run.sh"],
    );
    let links = held.resolved();
    let found = |path: &str| links.iter().find(|link| link.path == Path::new(path));
    let hooks = found(".claude/hooks").expect("the hooks link resolves");
    assert_eq!(hooks.target, Path::new("vendor/tools/hooks"));
    assert_eq!(
        hooks.passed,
        ["tools", "vendor", "vendor/tools"].map(PathBuf::from)
    );
    assert!(hooks.tree);
    let skills = found(".claude/skills").expect("the skills link resolves");
    assert_eq!(skills.target, Path::new("skills"));
    assert_eq!(skills.passed, [PathBuf::from("shared")]);
}

#[test]
fn a_link_that_leaves_the_checkout_or_loops_resolves_to_nothing() {
    let held = tree(
        &[
            (".claude/hooks", "../../elsewhere"),
            (".claude/settings.json", "/etc/settings.json"),
            (".claude/agents", "../.claude/agents"),
            (".claude/root", ".."),
        ],
        &[],
    );
    assert_eq!(held.resolved(), Vec::new());
}

#[test]
fn a_link_at_a_path_a_cli_loads_protects_its_target_for_that_reason() {
    let set = grown(
        fixed(),
        &tree(
            &[
                (".claude/hooks", "../tools/hooks"),
                ("CLAUDE.md", "docs/instructions.md"),
                ("docs/guide", "../tools/guide"),
            ],
            &["tools/hooks/run.sh", "docs/instructions.md"],
        ),
    );
    assert_eq!(
        reason(&set, "tools/hooks/run.sh"),
        Some(Protection::AgentConfig)
    );
    let instructions = reason(&set, "docs/instructions.md");
    assert_eq!(instructions, Some(Protection::AgentInstructions));
    assert!(instructions.is_some_and(Protection::person_may_grant));
    assert_eq!(reason(&set, "tools/guide/a.md"), None);
    assert_eq!(reason(&set, "tools/notes.md"), None);
}

/// A path the resolution walks through is held as itself: a link put in
/// its place is refused, what lies under it is not, and no session is told
/// of it.
#[test]
fn every_path_on_the_way_is_held_as_itself_and_told_to_no_session() {
    let set = grown(
        fixed(),
        &tree(
            &[
                (".claude/hooks", "../tools/hooks"),
                ("tools", "vendor/tools"),
            ],
            &["vendor/tools/hooks/run.sh", "vendor/tools/lint.sh"],
        ),
    );
    for walked in ["tools", "vendor", "vendor/tools", "vendor/tools/hooks"] {
        assert_eq!(
            reason(&set, walked),
            Some(Protection::AgentConfig),
            "{walked}"
        );
    }
    assert_eq!(reason(&set, "vendor/tools/lint.sh"), None);
    let told: Vec<String> = set.every().iter().map(|glob| glob.to_string()).collect();
    assert!(
        told.contains(&"vendor/tools/hooks/**".to_string()),
        "{told:?}"
    );
    assert!(!told.contains(&"tools".to_string()), "{told:?}");
    assert!(!told.contains(&".claude".to_string()), "{told:?}");
}

/// The root of a tree a CLI loads is held as itself even with no link
/// there, and a link committed there carries the tree to its target.
#[test]
fn a_link_at_the_root_of_a_tree_a_cli_loads_carries_the_tree_to_its_target() {
    assert_eq!(reason(&fixed(), ".codex"), Some(Protection::AgentConfig));
    let set = grown(
        fixed(),
        &tree(&[(".codex", "config/codex")], &["config/codex/config.toml"]),
    );
    assert_eq!(
        reason(&set, "config/codex/config.toml"),
        Some(Protection::AgentConfig)
    );
    assert_eq!(reason(&set, "config/codex"), Some(Protection::AgentConfig));
    assert_eq!(reason(&set, "config/other.toml"), None);
}

/// A link above a tree a CLI loads carries only that tree, beneath the
/// link's target: the rest of the target is not loaded through it.
#[test]
fn a_link_above_a_tree_a_cli_loads_carries_only_that_tree() {
    let set = grown(fixed(), &tree(&[(".agents", "shared/agents")], &[]));
    assert_eq!(
        reason(&set, "shared/agents/skills/x/SKILL.md"),
        Some(Protection::AgentConfig)
    );
    assert_eq!(
        reason(&set, "shared/agents/skills"),
        Some(Protection::AgentConfig)
    );
    assert_eq!(reason(&set, "shared/agents"), Some(Protection::AgentConfig));
    assert_eq!(reason(&set, "shared/agents/notes.md"), None);
}

/// A link inside what a followed link leads to is loaded through as well,
/// so it is followed in its turn.
#[test]
fn a_link_inside_a_followed_target_is_followed_in_its_turn() {
    let held = tree(
        &[
            (".claude/hooks", "../tools/hooks"),
            ("tools/hooks/pre.sh", "../../scripts/pre.sh"),
            ("docs/latest", "v2"),
        ],
        &["scripts/pre.sh"],
    );
    let set = fixed();
    let loaded: Vec<PathBuf> = set
        .loaded(held.resolved())
        .unwrap()
        .into_iter()
        .map(|link| link.path)
        .collect();
    assert_eq!(
        loaded,
        [".claude/hooks", "tools/hooks/pre.sh"].map(PathBuf::from)
    );
    let set = grown(set, &held);
    assert_eq!(
        reason(&set, "scripts/pre.sh"),
        Some(Protection::AgentConfig)
    );
    assert_eq!(reason(&set, "scripts/other.sh"), None);
}

#[test]
fn a_link_the_project_denies_too_still_carries_the_reason_a_cli_loads_it_for() {
    let set = Protected::new(&[ScopeGlob::from(".claude/**")], &[]).unwrap();
    let set = grown(set, &tree(&[(".claude/hooks", "../tools/hooks")], &[]));
    assert_eq!(
        reason(&set, "tools/hooks/run.sh"),
        Some(Protection::AgentConfig)
    );
}

#[test]
fn a_followed_tree_is_one_the_sandbox_denies_and_a_followed_file_is_not() {
    let set = grown(
        fixed(),
        &tree(
            &[
                (".claude/hooks", "../tools/hooks"),
                (".mcp.json", "config/mcp.json"),
            ],
            &["config/mcp.json"],
        ),
    );
    let trees: Vec<String> = set
        .every()
        .iter()
        .filter_map(|glob| glob.whole_tree().map(str::to_string))
        .collect();
    assert!(trees.contains(&"tools/hooks".to_string()), "{trees:?}");
    assert!(!trees.contains(&"config/mcp.json".to_string()), "{trees:?}");
    assert_eq!(
        reason(&set, "config/mcp.json"),
        Some(Protection::AgentConfig)
    );
}

/// A grant names the file a session writes. Granting what a link
/// resolves to lifts it once the set follows the link; granting only the
/// link's path leaves the target held.
#[test]
fn a_grant_of_what_a_link_resolves_to_survives_the_following() {
    let set = fixed();
    assert_eq!(set.clone().following(&[]).unwrap(), set);
    let link = LoadedLink {
        path: "CLAUDE.md".into(),
        target: "docs/claude.md".into(),
        passed: vec!["docs".into()],
        tree: false,
    };
    let granted = set
        .lifting(&[ScopeGlob::from("docs/claude.md")])
        .following(std::slice::from_ref(&link))
        .unwrap();
    assert_eq!(reason(&granted, "docs/claude.md"), None);
    let link_only = set
        .lifting(&[ScopeGlob::from("CLAUDE.md")])
        .following(&[link])
        .unwrap();
    assert_eq!(
        reason(&link_only, "docs/claude.md"),
        Some(Protection::AgentInstructions)
    );
}

/// A reason that reaches a link in a later round is carried too: what a link
/// inside a followed configuration tree resolves to is configuration, though
/// the link is an instruction file as well.
#[test]
fn a_stricter_reason_that_reaches_a_link_later_is_carried_too() {
    let held = tree(
        &[
            (".claude/hooks", "../shared"),
            ("shared/CLAUDE.md", "../docs/c.md"),
        ],
        &["docs/c.md"],
    );
    let set = grown(fixed(), &held);
    assert_eq!(reason(&set, "docs/c.md"), Some(Protection::AgentConfig));
}
