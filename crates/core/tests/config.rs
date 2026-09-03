use std::collections::BTreeMap;
use std::path::PathBuf;

use yunta_core::{
    permission_layer_conflicts, AdapterSettings, CommandPermissions, ConfigLayer, DefaultsConfig,
    ExecutorKind, ExecutorRegistration, GitHubRepo, Isolation, LimitsConfig, McpServerConfig,
    NetworkPermissions, OnInterrupt, PackExecutorPolicy, PackPermissions, PathsConfig,
    PermissionsConfig, RunnerCandidate, SkillsConfig, StorageConfig,
};

fn candidate(adapter: &str, model: &str) -> RunnerCandidate {
    RunnerCandidate {
        adapter: adapter.into(),
        model: model.into(),
        agent: None,
    }
}

#[test]
fn repo_replaces_a_runners_array_wholesale_instead_of_concatenating() {
    let org = ConfigLayer {
        runners: Some(BTreeMap::from([
            ("planner".into(), vec![candidate("codex", "gpt-5-codex")]),
            ("reviewer".into(), vec![candidate("codex", "gpt-5-codex")]),
        ])),
        ..Default::default()
    };
    let repo = ConfigLayer {
        runners: Some(BTreeMap::from([(
            "planner".into(),
            vec![candidate("claude-code", "claude-opus-4-8")],
        )])),
        ..Default::default()
    };

    let merged = ConfigLayer::merge_layers([org, repo]);
    let runners = merged.runners.unwrap();

    // repo overrides `planner` entirely — org's codex candidate is gone,
    // not appended to.
    assert_eq!(
        runners["planner"],
        vec![candidate("claude-code", "claude-opus-4-8")]
    );
    // `reviewer`, untouched by repo, survives from org unchanged.
    assert_eq!(runners["reviewer"], vec![candidate("codex", "gpt-5-codex")]);
}

#[test]
fn user_wins_over_org_for_a_field_repo_never_sets() {
    let org = ConfigLayer {
        adapters: Some(BTreeMap::from([(
            "claude-code".into(),
            AdapterSettings {
                binary: Some(PathBuf::from("/usr/bin/claude")),
                adapter_settings: None,
            },
        )])),
        ..Default::default()
    };
    let user = ConfigLayer {
        adapters: Some(BTreeMap::from([(
            "claude-code".into(),
            AdapterSettings {
                binary: Some(PathBuf::from("~/.local/bin/claude")),
                adapter_settings: None,
            },
        )])),
        ..Default::default()
    };
    let repo = ConfigLayer::default();

    let merged = ConfigLayer::merge_layers([org, user, repo]);
    let adapters = merged.adapters.unwrap();

    assert_eq!(
        adapters["claude-code"].binary,
        Some(PathBuf::from("~/.local/bin/claude"))
    );
}

#[test]
fn storage_fields_deep_merge_across_layers() {
    let org = ConfigLayer {
        storage: Some(StorageConfig {
            path: None,
            retention_days: Some(90),
        }),
        ..Default::default()
    };
    let user = ConfigLayer {
        storage: Some(StorageConfig {
            path: Some(PathBuf::from("~/.yunta/yunta.db")),
            retention_days: None,
        }),
        ..Default::default()
    };

    let merged = ConfigLayer::merge_layers([org, user]);
    let storage = merged.storage.unwrap();

    // Neither layer alone had both fields — merging combines them
    // field-by-field rather than one layer replacing the other's block.
    assert_eq!(storage.path, Some(PathBuf::from("~/.yunta/yunta.db")));
    assert_eq!(storage.retention_days, Some(90));
}

#[test]
fn repo_over_user_over_org_precedence_holds_for_paths() {
    let org = ConfigLayer {
        paths: Some(PathsConfig {
            runs: Some(PathBuf::from("/org/default/runs")),
            worktrees: Some(PathBuf::from("/org/default/worktrees")),
        }),
        ..Default::default()
    };
    let user = ConfigLayer {
        paths: Some(PathsConfig {
            runs: Some(PathBuf::from("~/.yunta/runs")),
            worktrees: None,
        }),
        ..Default::default()
    };
    let repo = ConfigLayer::default();

    let merged = ConfigLayer::merge_layers([org, user, repo]);
    let paths = merged.paths.unwrap();

    assert_eq!(paths.runs, Some(PathBuf::from("~/.yunta/runs")));
    assert_eq!(
        paths.worktrees,
        Some(PathBuf::from("/org/default/worktrees"))
    );
}

#[test]
fn a_group_absent_from_every_layer_stays_none() {
    let merged = ConfigLayer::merge_layers([ConfigLayer::default(), ConfigLayer::default()]);
    assert_eq!(merged.runners, None);
    assert_eq!(merged.adapters, None);
    assert_eq!(merged.storage, None);
    assert_eq!(merged.paths, None);
}

#[test]
fn parses_the_reference_config_groups_in_scope() {
    let yaml = r#"
runners:
  planner:
    - { adapter: claude-code, model: claude-opus-4-8 }
    - { adapter: codex, model: gpt-5-codex }
  reviewer:
    - { adapter: claude-code, model: claude-sonnet-4-6, agent: benito }

adapters:
  claude-code:
    binary: ~/.local/bin/claude

storage:
  path: ~/.yunta/yunta.db
  retention_days: 90

paths:
  runs: ~/.yunta/runs
  worktrees: ~/.yunta/worktrees
"#;

    let layer: ConfigLayer = serde_yaml::from_str(yaml).expect("reference config should parse");

    assert_eq!(
        layer.runners.as_ref().unwrap()["reviewer"][0]
            .agent
            .as_ref()
            .map(|agent| agent.as_str()),
        Some("benito")
    );
    assert_eq!(
        layer.adapters.as_ref().unwrap()["claude-code"].binary,
        Some(PathBuf::from("~/.local/bin/claude"))
    );
    assert_eq!(layer.storage.as_ref().unwrap().retention_days, Some(90));
    assert_eq!(
        layer.paths.as_ref().unwrap().runs,
        Some(PathBuf::from("~/.yunta/runs"))
    );
}

#[test]
fn isolation_defaults_to_worktree_when_unset() {
    let layer = ConfigLayer::default();
    assert_eq!(layer.resolved_isolation(), Isolation::Worktree);
}

#[test]
fn isolation_parses_from_defaults_and_none_is_a_real_choice() {
    let layer: ConfigLayer = serde_yaml::from_str("defaults:\n  isolation: none\n").unwrap();
    assert_eq!(layer.resolved_isolation(), Isolation::None);
}

#[test]
fn an_unknown_isolation_value_is_a_parse_error_not_silently_ignored() {
    let err =
        serde_yaml::from_str::<ConfigLayer>("defaults:\n  isolation: container\n").unwrap_err();
    assert!(err.to_string().contains("container") || err.to_string().contains("unknown"));
}

#[test]
fn repo_isolation_overrides_org_isolation() {
    let org = ConfigLayer {
        defaults: Some(DefaultsConfig {
            isolation: Some(Isolation::None),
            ..Default::default()
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        defaults: Some(DefaultsConfig {
            isolation: Some(Isolation::Worktree),
            ..Default::default()
        }),
        ..Default::default()
    };
    let merged = ConfigLayer::merge_layers([org, repo]);
    assert_eq!(merged.resolved_isolation(), Isolation::Worktree);
}

#[test]
fn max_parallel_nodes_defaults_to_1_when_unset() {
    let layer = ConfigLayer::default();
    assert_eq!(layer.resolved_max_parallel_nodes(), 1);
}

#[test]
fn max_parallel_nodes_parses_from_defaults() {
    let layer: ConfigLayer = serde_yaml::from_str("defaults:\n  max_parallel_nodes: 4\n").unwrap();
    assert_eq!(layer.resolved_max_parallel_nodes(), 4);
}

#[test]
fn repo_max_parallel_nodes_overrides_org_max_parallel_nodes() {
    let org = ConfigLayer {
        defaults: Some(DefaultsConfig {
            max_parallel_nodes: Some(2),
            ..Default::default()
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        defaults: Some(DefaultsConfig {
            max_parallel_nodes: Some(8),
            ..Default::default()
        }),
        ..Default::default()
    };
    let merged = ConfigLayer::merge_layers([org, repo]);
    assert_eq!(merged.resolved_max_parallel_nodes(), 8);
}

#[test]
fn on_interrupt_defaults_to_restart_node_when_unset() {
    let layer = ConfigLayer::default();
    assert_eq!(layer.resolved_on_interrupt(), OnInterrupt::RestartNode);
}

#[test]
fn on_interrupt_parses_fail_if_uncertain_from_defaults() {
    let layer: ConfigLayer =
        serde_yaml::from_str("defaults:\n  on_interrupt: fail_if_uncertain\n").unwrap();
    assert_eq!(layer.resolved_on_interrupt(), OnInterrupt::FailIfUncertain);
}

#[test]
fn permissions_parses_the_reference_config_shape() {
    let yaml = r#"
permissions:
  commands:
    deny: ["curl * | *", "sudo *"]
  packs:
    executors: prompt
    publishers: { allow: [acme] }
  network:
    default: true
"#;
    let layer: ConfigLayer = serde_yaml::from_str(yaml).unwrap();
    let perms = layer.permissions.unwrap();
    let commands = perms.commands.unwrap();
    assert_eq!(commands.deny, vec!["curl * | *", "sudo *"]);
    assert!(commands.allow.is_empty(), "absent allow = denylist mode");
    let packs = perms.packs.unwrap();
    assert_eq!(packs.executors, Some(PackExecutorPolicy::Prompt));
    assert_eq!(packs.publishers.unwrap().allow, vec!["acme"]);
    assert!(perms.network.unwrap().default);
}

#[test]
fn permissions_merge_unions_deny_lists_instead_of_replacing() {
    // The inversion: adding denies is narrowing, always legal — unlike
    // every other config array, deny lists accumulate across layers.
    let org = ConfigLayer {
        permissions: Some(PermissionsConfig {
            commands: Some(CommandPermissions {
                deny: vec!["sudo *".to_string()],
                allow: vec![],
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        permissions: Some(PermissionsConfig {
            commands: Some(CommandPermissions {
                deny: vec!["rm -rf *".to_string()],
                allow: vec![],
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = ConfigLayer::merge_layers([org, repo]);
    let commands = merged.permissions.unwrap().commands.unwrap();
    assert_eq!(commands.deny, vec!["sudo *", "rm -rf *"]);
}

#[test]
fn permissions_merge_keeps_the_stricter_executors_policy_and_network_default() {
    let org = ConfigLayer {
        permissions: Some(PermissionsConfig {
            packs: Some(PackPermissions {
                executors: Some(PackExecutorPolicy::Deny),
                publishers: None,
            }),
            network: Some(NetworkPermissions { default: false }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        permissions: Some(PermissionsConfig {
            packs: Some(PackPermissions {
                executors: Some(PackExecutorPolicy::Allow),
                publishers: None,
            }),
            network: Some(NetworkPermissions { default: true }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = ConfigLayer::merge_layers([org, repo]);
    let perms = merged.permissions.unwrap();
    assert_eq!(
        perms.packs.unwrap().executors,
        Some(PackExecutorPolicy::Deny),
        "repo cannot loosen org's executors: deny"
    );
    assert!(
        !perms.network.unwrap().default,
        "repo cannot re-enable network org turned off by default"
    );
}

#[test]
fn a_lower_layer_re_allowing_an_org_denied_pattern_is_a_named_conflict() {
    let org = ConfigLayer {
        permissions: Some(PermissionsConfig {
            commands: Some(CommandPermissions {
                deny: vec!["sudo *".to_string()],
                allow: vec![],
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        permissions: Some(PermissionsConfig {
            commands: Some(CommandPermissions {
                deny: vec![],
                allow: vec!["sudo *".to_string()],
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let conflicts = permission_layer_conflicts(&[("org", &org), ("repo", &repo)]);
    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0].contains("repo"), "must cite the lower layer");
    assert!(conflicts[0].contains("org"), "must cite the ceiling layer");
    assert!(conflicts[0].contains("sudo *"), "must cite the pattern");
}

#[test]
fn layers_that_only_narrow_produce_no_conflicts() {
    let org = ConfigLayer {
        permissions: Some(PermissionsConfig {
            commands: Some(CommandPermissions {
                deny: vec!["sudo *".to_string()],
                allow: vec![],
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        permissions: Some(PermissionsConfig {
            commands: Some(CommandPermissions {
                deny: vec!["curl *".to_string()],
                allow: vec![],
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let conflicts = permission_layer_conflicts(&[("org", &org), ("repo", &repo)]);
    assert!(
        conflicts.is_empty(),
        "adding denies is narrowing: {conflicts:?}"
    );
}

#[test]
fn skills_executors_parses_name_kind_and_path() {
    let yaml = "skills:\n  executors:\n    - { name: coverage-gate, kind: binary, path: .yunta/bin/coverage-gate }\n";
    let layer: ConfigLayer = serde_yaml::from_str(yaml).unwrap();
    let executors = layer.skills.unwrap().executors;
    assert_eq!(
        executors,
        vec![ExecutorRegistration {
            name: "coverage-gate".into(),
            kind: ExecutorKind::Binary,
            path: PathBuf::from(".yunta/bin/coverage-gate"),
        }]
    );
}

#[test]
fn an_unknown_executor_kind_fails_to_parse() {
    let yaml = "skills:\n  executors:\n    - { name: x, kind: wasm, path: x }\n";
    let result: Result<ConfigLayer, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "`wasm` is reserved for later but not built yet — must not silently parse as binary"
    );
}

#[test]
fn repo_replaces_skills_executors_wholesale_instead_of_concatenating() {
    let org = ConfigLayer {
        skills: Some(SkillsConfig {
            executors: vec![ExecutorRegistration {
                name: "org-tool".into(),
                kind: ExecutorKind::Binary,
                path: PathBuf::from("org-tool"),
            }],
            paths: Vec::new(),
            always: Vec::new(),
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        skills: Some(SkillsConfig {
            executors: vec![ExecutorRegistration {
                name: "repo-tool".into(),
                kind: ExecutorKind::Binary,
                path: PathBuf::from("repo-tool"),
            }],
            paths: Vec::new(),
            always: Vec::new(),
        }),
        ..Default::default()
    };

    let merged = ConfigLayer::merge_layers([org, repo]);
    let executors = merged.skills.unwrap().executors;
    assert_eq!(executors.len(), 1);
    assert_eq!(executors[0].name, "repo-tool");
}

#[test]
fn mcp_servers_parses_the_reference_config_shape() {
    let yaml = r#"
mcp_servers:
  internal-docs: { url: "https://docs.interna.example/mcp", auth_env: DOCS_TOKEN }
"#;
    let layer: ConfigLayer = serde_yaml::from_str(yaml).unwrap();
    let servers = layer.mcp_servers.unwrap();
    assert_eq!(
        servers["internal-docs"].url,
        "https://docs.interna.example/mcp"
    );
    assert_eq!(
        servers["internal-docs"].auth_env.as_deref(),
        Some("DOCS_TOKEN")
    );
}

#[test]
fn mcp_servers_auth_env_defaults_to_absent_for_a_public_server() {
    let yaml = r#"
mcp_servers:
  public-docs: { url: "https://docs.example.com/mcp" }
"#;
    let layer: ConfigLayer = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(layer.mcp_servers.unwrap()["public-docs"].auth_env, None);
}

#[test]
fn repo_replaces_an_mcp_server_entry_wholesale_others_survive_from_org() {
    let org = ConfigLayer {
        mcp_servers: Some(BTreeMap::from([
            (
                "internal-docs".to_string(),
                McpServerConfig {
                    url: "https://org.example.com/mcp".to_string(),
                    auth_env: Some("ORG_TOKEN".to_string()),
                },
            ),
            (
                "other".to_string(),
                McpServerConfig {
                    url: "https://other.example.com/mcp".to_string(),
                    auth_env: None,
                },
            ),
        ])),
        ..Default::default()
    };
    let repo = ConfigLayer {
        mcp_servers: Some(BTreeMap::from([(
            "internal-docs".to_string(),
            McpServerConfig {
                url: "http://localhost:8000/mcp".to_string(),
                auth_env: None,
            },
        )])),
        ..Default::default()
    };

    let merged = ConfigLayer::merge_layers([org, repo]).mcp_servers.unwrap();
    assert_eq!(merged["internal-docs"].url, "http://localhost:8000/mcp");
    assert_eq!(merged["internal-docs"].auth_env, None);
    assert_eq!(merged["other"].url, "https://other.example.com/mcp");
}

#[test]
fn limits_parses_the_reference_config_shape() {
    let yaml = r#"
limits:
  max_tokens_per_run: 2000000
  max_loop_iterations: 12
  max_concurrent_runs: 3
  max_workflow_depth: 2
  max_artifact_bytes: 10485760
  inline_context_bytes: 32000
"#;
    let layer: ConfigLayer = serde_yaml::from_str(yaml).unwrap();
    let limits = layer.limits.unwrap();
    assert_eq!(limits.max_tokens_per_run, Some(2_000_000));
    assert_eq!(limits.max_loop_iterations, Some(12));
    assert_eq!(limits.max_concurrent_runs, Some(3));
    assert_eq!(limits.max_workflow_depth, Some(2));
    assert_eq!(limits.max_artifact_bytes, Some(10_485_760));
    assert_eq!(limits.inline_context_bytes, Some(32_000));
}

#[test]
fn an_underscored_limit_literal_is_a_loud_parse_error_not_a_silent_string() {
    // serde_yaml (YAML 1.2) resolves `2_000_000` as a string — the
    // canonical form is `2000000`; anything else must fail the parse
    // rather than quietly become an unlimited run.
    let result: Result<ConfigLayer, _> =
        serde_yaml::from_str("limits:\n  max_tokens_per_run: 2_000_000\n");
    assert!(result.is_err(), "underscored literal must not parse");
}

#[test]
fn limits_fields_deep_merge_across_layers_with_repo_winning() {
    let org = ConfigLayer {
        limits: Some(LimitsConfig {
            max_tokens_per_run: Some(1_000_000),
            max_loop_iterations: Some(20),
            ..Default::default()
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        limits: Some(LimitsConfig {
            max_tokens_per_run: Some(2_000_000),
            inline_context_bytes: Some(16_000),
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = ConfigLayer::merge_layers([org, repo]).limits.unwrap();
    // Normal precedence (repo > org) — the inverted ceiling merge is
    // exclusive to `permissions`; limits are budgets, not permissions.
    assert_eq!(merged.max_tokens_per_run, Some(2_000_000));
    assert_eq!(merged.max_loop_iterations, Some(20));
    assert_eq!(merged.inline_context_bytes, Some(16_000));
    assert_eq!(merged.max_concurrent_runs, None);
}

#[test]
fn loop_iterations_and_inline_context_resolve_to_reference_defaults_when_unset() {
    let layer = ConfigLayer::default();
    assert_eq!(layer.resolved_max_loop_iterations(), 12);
    assert_eq!(layer.resolved_inline_context_bytes(), 32_000);
}

#[test]
fn project_parses_the_reference_config_shape() {
    let yaml = r#"
project:
  name: mi-repo
  base_branch: main
  branch_prefix: yunta/
"#;
    let layer: ConfigLayer = serde_yaml::from_str(yaml).unwrap();
    let project = layer.project.unwrap();
    assert_eq!(project.name.as_deref(), Some("mi-repo"));
    assert_eq!(project.base_branch.as_deref(), Some("main"));
    assert_eq!(project.branch_prefix.as_deref(), Some("yunta/"));
}

#[test]
fn repo_overrides_only_the_project_fields_it_sets() {
    let org = ConfigLayer {
        project: Some(yunta_core::ProjectConfig {
            name: Some("org-default".to_string()),
            base_branch: Some("main".to_string()),
            branch_prefix: None,
        }),
        ..Default::default()
    };
    let repo = ConfigLayer {
        project: Some(yunta_core::ProjectConfig {
            name: Some("mi-repo".to_string()),
            base_branch: None,
            branch_prefix: None,
        }),
        ..Default::default()
    };

    let merged = ConfigLayer::merge_layers([org, repo]).project.unwrap();
    assert_eq!(merged.name.as_deref(), Some("mi-repo"));
    assert_eq!(merged.base_branch.as_deref(), Some("main"));
    assert_eq!(merged.branch_prefix, None);
}

// --- Layered ceiling for scope_expansion -------------------------------------

#[test]
fn scope_expansion_ceiling_merges_to_the_strictest_layer() {
    let org: ConfigLayer =
        serde_yaml::from_str("permissions: { scope_expansion: { max_mode: ask } }").unwrap();
    let repo: ConfigLayer =
        serde_yaml::from_str("permissions: { scope_expansion: { max_mode: deny } }").unwrap();
    // org → repo (most specific last): the harder ceiling wins.
    let merged = ConfigLayer::merge_layers(vec![org.clone(), repo]);
    assert_eq!(
        merged
            .permissions
            .unwrap()
            .scope_expansion
            .unwrap()
            .max_mode,
        yunta_core::ScopeExpansionMode::Deny
    );

    // The other way around the ceiling still holds: a softer lower
    // layer never wins the merge.
    let soft_repo: ConfigLayer =
        serde_yaml::from_str("permissions: { scope_expansion: { max_mode: rules } }").unwrap();
    let merged = ConfigLayer::merge_layers(vec![org, soft_repo]);
    assert_eq!(
        merged
            .permissions
            .unwrap()
            .scope_expansion
            .unwrap()
            .max_mode,
        yunta_core::ScopeExpansionMode::Ask
    );
}

#[test]
fn a_layer_softening_the_scope_expansion_ceiling_is_a_conflict() {
    let org: ConfigLayer =
        serde_yaml::from_str("permissions: { scope_expansion: { max_mode: ask } }").unwrap();
    let repo: ConfigLayer =
        serde_yaml::from_str("permissions: { scope_expansion: { max_mode: rules } }").unwrap();
    let conflicts = yunta_core::permission_layer_conflicts(&[("org", &org), ("repo", &repo)]);
    assert!(
        conflicts
            .iter()
            .any(|c| c.contains("scope_expansion") && c.contains("repo") && c.contains("org")),
        "got: {conflicts:?}"
    );

    // Hardening is narrowing — never a conflict.
    let hard: ConfigLayer =
        serde_yaml::from_str("permissions: { scope_expansion: { max_mode: deny } }").unwrap();
    assert!(yunta_core::permission_layer_conflicts(&[("org", &org), ("repo", &hard)]).is_empty());
}

// --- `~` in paths ------------------------------------------------------------

#[test]
fn a_leading_tilde_expands_against_the_given_home_in_every_path_field() {
    let mut layer: yunta_core::ConfigLayer = yunta_core::yaml::parse(
        "adapters:\n  claude-code: { binary: ~/bin/claude }\nstorage: { path: ~/state/yunta.db }\n\
         paths: { runs: ~/runs, worktrees: \"~\" }\nskills: { paths: [~/skills, ./local] }\n",
    )
    .unwrap();
    layer
        .expand_home(Some(std::path::Path::new("/home/ana")))
        .unwrap();
    assert_eq!(
        layer.adapters.unwrap()["claude-code"].binary.as_deref(),
        Some(std::path::Path::new("/home/ana/bin/claude"))
    );
    assert_eq!(
        layer.storage.unwrap().path.as_deref(),
        Some(std::path::Path::new("/home/ana/state/yunta.db"))
    );
    let paths = layer.paths.unwrap();
    assert_eq!(
        paths.runs.as_deref(),
        Some(std::path::Path::new("/home/ana/runs"))
    );
    assert_eq!(
        paths.worktrees.as_deref(),
        Some(std::path::Path::new("/home/ana"))
    );
    let skills = layer.skills.unwrap();
    assert_eq!(skills.paths[0], std::path::Path::new("/home/ana/skills"));
    assert_eq!(skills.paths[1], std::path::Path::new("./local"));
}

#[test]
fn a_tilde_with_no_home_is_an_error_that_names_the_field() {
    let mut layer: yunta_core::ConfigLayer =
        yunta_core::yaml::parse("storage: { path: ~/state/yunta.db }\n").unwrap();
    let err = layer.expand_home(None).unwrap_err();
    assert!(matches!(
        &err,
        yunta_core::HomeExpansionError::NoHome { field, .. } if field == "storage.path"
    ));
    let text = err.to_string();
    assert!(
        text.contains("storage.path") && text.contains("HOME"),
        "{text}"
    );
}

#[test]
fn another_users_home_is_refused_naming_the_field() {
    let mut layer: yunta_core::ConfigLayer =
        yunta_core::yaml::parse("paths: { runs: ~ana/runs }\n").unwrap();
    let err = layer
        .expand_home(Some(std::path::Path::new("/home/me")))
        .unwrap_err();
    assert!(matches!(
        &err,
        yunta_core::HomeExpansionError::OtherUser { field, .. } if field == "paths.runs"
    ));
}

#[test]
fn a_github_repo_is_owner_slash_name() {
    let repo: GitHubRepo = "octo/widgets".parse().unwrap();
    assert_eq!(repo.owner(), "octo");
    assert_eq!(repo.name(), "widgets");
    assert_eq!(repo.to_string(), "octo/widgets");
    for bad in ["octo", "octo/", "/widgets", "octo/widgets/extra", ""] {
        assert!(
            bad.parse::<GitHubRepo>().is_err(),
            "{bad:?} is not a repository"
        );
    }

    let err = yunta_core::yaml::parse::<ConfigLayer>(
        "forge:\n  github:\n    repo: not-a-repo\n    token_env: GITHUB_TOKEN\n",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("forge.github.repo"),
        "the refusal names the path: {err}"
    );
}
