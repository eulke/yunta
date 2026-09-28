//! `requires:` validated against the local merged config —
//! pure comparison, no filesystem/PATH involved.

use std::collections::BTreeMap;
use yunta_core::{
    ConfigLayer, PackDeclares, PackManifest, PackRequires, RequiredRunner, RunnerCandidate,
    RunnerName,
};
use yunta_engine::check_pack_requires;

fn manifest(
    runners: Vec<RequiredRunner>,
    mcp_servers: Vec<&str>,
    commands: Vec<&str>,
) -> PackManifest {
    PackManifest {
        name: "review-pack".into(),
        publisher: "acme".into(),
        version: "1.0.0".to_string(),
        description: None,
        license: None,
        yunta_schema: None,
        requires: PackRequires {
            runners,
            mcp_servers: mcp_servers.into_iter().map(Into::into).collect(),
            commands: commands.into_iter().map(str::to_string).collect(),
        },
        declares: PackDeclares {
            permissions: yunta_core::NodePermissions::ReadOnly,
            network: false,
            executors: Vec::new(),
        },
        contents: Default::default(),
    }
}

#[test]
fn a_role_missing_from_runners_is_flagged() {
    let manifest = manifest(
        vec![RequiredRunner {
            name: "reviewer".into(),
            permissions: None,
        }],
        vec![],
        vec![],
    );
    let config = ConfigLayer::default();

    let gap = check_pack_requires(&manifest, &config);
    assert_eq!(gap.pack.to_string(), "acme/review-pack");
    assert_eq!(gap.missing_runners, vec![RunnerName::from("reviewer")]);
    assert!(!gap.is_satisfied());
}

#[test]
fn a_role_with_zero_candidates_is_also_flagged() {
    let manifest = manifest(
        vec![RequiredRunner {
            name: "reviewer".into(),
            permissions: None,
        }],
        vec![],
        vec![],
    );
    let config = ConfigLayer {
        runners: Some(BTreeMap::from([("reviewer".into(), Vec::new())])),
        ..Default::default()
    };

    let gap = check_pack_requires(&manifest, &config);
    assert_eq!(gap.missing_runners, vec![RunnerName::from("reviewer")]);
}

#[test]
fn a_role_with_at_least_one_candidate_resolves() {
    let manifest = manifest(
        vec![RequiredRunner {
            name: "reviewer".into(),
            permissions: None,
        }],
        vec![],
        vec![],
    );
    let config = ConfigLayer {
        runners: Some(BTreeMap::from([(
            "reviewer".into(),
            vec![RunnerCandidate {
                adapter: "claude-code".into(),
                model: "sonnet".into(),
                agent: None,
            }],
        )])),
        ..Default::default()
    };

    let gap = check_pack_requires(&manifest, &config);
    assert!(gap.missing_runners.is_empty());
    assert!(gap.is_satisfied());
}

#[test]
fn an_undefined_mcp_server_is_flagged() {
    let manifest = manifest(vec![], vec!["internal-docs"], vec![]);
    let config = ConfigLayer::default();

    let gap = check_pack_requires(&manifest, &config);
    assert_eq!(
        gap.missing_mcp_servers,
        vec![yunta_core::McpServerName::from("internal-docs")]
    );
    assert!(!gap.is_satisfied());
}

#[test]
fn a_defined_mcp_server_resolves() {
    let manifest = manifest(vec![], vec!["internal-docs"], vec![]);
    let config = ConfigLayer {
        mcp_servers: Some(BTreeMap::from([(
            "internal-docs".into(),
            yunta_core::McpServerConfig {
                url: "https://example.invalid".to_string(),
                auth_env: None,
            },
        )])),
        ..Default::default()
    };

    let gap = check_pack_requires(&manifest, &config);
    assert!(gap.missing_mcp_servers.is_empty());
    assert!(gap.is_satisfied());
}

#[test]
fn required_commands_pass_through_untouched_for_the_caller_to_check_on_path() {
    let manifest = manifest(vec![], vec![], vec!["gh", "cargo"]);
    let config = ConfigLayer::default();

    let gap = check_pack_requires(&manifest, &config);
    assert_eq!(
        gap.required_commands,
        vec!["gh".to_string(), "cargo".to_string()]
    );
    // Commands never affect is_satisfied() — that's yunta doctor's own
    // PATH lookup, not this pure function's job.
    assert!(gap.is_satisfied());
}

#[test]
fn a_fully_satisfied_pack_reports_nothing_missing() {
    let manifest = manifest(vec![], vec![], vec![]);
    let config = ConfigLayer::default();

    let gap = check_pack_requires(&manifest, &config);
    assert!(gap.is_satisfied());
    assert!(gap.missing_runners.is_empty());
    assert!(gap.missing_mcp_servers.is_empty());
}

/// What a gap leaves a run without, as the refusals `check` gives: a
/// command counts only when the caller's lookup cannot find it.
#[test]
fn a_gap_is_refused_for_each_thing_the_run_would_lack() {
    let manifest = manifest(
        vec![RequiredRunner {
            name: RunnerName::from("reviewer"),
            permissions: None,
        }],
        vec!["internal-docs"],
        vec!["gh", "cargo"],
    );
    let gap = check_pack_requires(&manifest, &ConfigLayer::default());

    let refused: Vec<String> = gap
        .unmet(&|command| command == "cargo")
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(refused.len(), 3, "{refused:?}");
    assert!(refused[0].starts_with("pack `acme/review-pack` requires runner `reviewer`"));
    assert!(refused[1].starts_with("pack `acme/review-pack` requires MCP server `internal-docs`"));
    assert!(refused[2].starts_with("pack `acme/review-pack` requires command `gh`"));

    let satisfied = check_pack_requires(
        &self::manifest(vec![], vec![], vec!["cargo"]),
        &ConfigLayer::default(),
    );
    assert!(satisfied.unmet(&|_| true).is_empty());
}
