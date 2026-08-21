//! `requires:` validated against the local merged config (RFC-0002 §3,
//! T11.6) — pure comparison, no filesystem/PATH involved.

use std::collections::HashMap;

use yunta_core::{
    ConfigLayer, PackDeclares, PackManifest, PackRequires, RequiredRole, RunnerCandidate,
};
use yunta_engine::check_pack_requires;

fn manifest(roles: Vec<RequiredRole>, mcp_servers: Vec<&str>, commands: Vec<&str>) -> PackManifest {
    PackManifest {
        name: "review-pack".to_string(),
        publisher: "acme".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        license: None,
        yunta_schema: None,
        requires: PackRequires {
            roles,
            mcp_servers: mcp_servers.into_iter().map(str::to_string).collect(),
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
        vec![RequiredRole {
            name: "reviewer".to_string(),
            permissions: None,
        }],
        vec![],
        vec![],
    );
    let config = ConfigLayer::default();

    let gap = check_pack_requires(&manifest, &config);
    assert_eq!(gap.pack, "acme/review-pack");
    assert_eq!(gap.missing_roles, vec!["reviewer".to_string()]);
    assert!(!gap.is_satisfied());
}

#[test]
fn a_role_with_zero_candidates_is_also_flagged() {
    let manifest = manifest(
        vec![RequiredRole {
            name: "reviewer".to_string(),
            permissions: None,
        }],
        vec![],
        vec![],
    );
    let config = ConfigLayer {
        runners: Some(HashMap::from([("reviewer".to_string(), Vec::new())])),
        ..Default::default()
    };

    let gap = check_pack_requires(&manifest, &config);
    assert_eq!(gap.missing_roles, vec!["reviewer".to_string()]);
}

#[test]
fn a_role_with_at_least_one_candidate_resolves() {
    let manifest = manifest(
        vec![RequiredRole {
            name: "reviewer".to_string(),
            permissions: None,
        }],
        vec![],
        vec![],
    );
    let config = ConfigLayer {
        runners: Some(HashMap::from([(
            "reviewer".to_string(),
            vec![RunnerCandidate {
                adapter: "claude-code".to_string(),
                model: "sonnet".to_string(),
                agent: None,
            }],
        )])),
        ..Default::default()
    };

    let gap = check_pack_requires(&manifest, &config);
    assert!(gap.missing_roles.is_empty());
    assert!(gap.is_satisfied());
}

#[test]
fn an_undefined_mcp_server_is_flagged() {
    let manifest = manifest(vec![], vec!["internal-docs"], vec![]);
    let config = ConfigLayer::default();

    let gap = check_pack_requires(&manifest, &config);
    assert_eq!(gap.missing_mcp_servers, vec!["internal-docs".to_string()]);
    assert!(!gap.is_satisfied());
}

#[test]
fn a_defined_mcp_server_resolves() {
    let manifest = manifest(vec![], vec!["internal-docs"], vec![]);
    let config = ConfigLayer {
        mcp_servers: Some(HashMap::from([(
            "internal-docs".to_string(),
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
    assert!(gap.missing_roles.is_empty());
    assert!(gap.missing_mcp_servers.is_empty());
}
