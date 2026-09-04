//! End to end: `permissions.packs` actually
//! enforced by `pack add`/`pack update` — the publisher allow-list and
//! the `executors: allow|prompt|deny` policy, against the real compiled
//! binary. Parsing and merging the field alone doesn't enforce anything;
//! this suite proves the policy is actually applied.

use std::path::Path;

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in, INITIAL_BRANCH};

/// A minimal pack from `publisher`, with or without a declared executor.
fn write_pack(dir: &Path, publisher: &str, version: &str, executors: &str) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        format!(
            "name: tools-pack\n\
             publisher: {publisher}\n\
             version: {version}\n\
             declares:\n  permissions: read-only\n  network: false\n  executors: [{executors}]\n\
             contents:\n  workflows: [workflows/noop.yaml]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/noop.yaml"),
        "name: noop\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", version]);
}

fn setup_project(config: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(repo.join(".yunta/config.yaml"), config).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "initial"]);
    let home = root.path().join("state");
    (root, repo, home)
}

fn upstream(publisher: &str, executors: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    write_pack(dir.path(), publisher, "1.0.0", executors);
    dir
}

#[test]
fn add_refuses_a_publisher_outside_a_non_empty_allow_list() {
    let (_root, repo, home) =
        setup_project("permissions:\n  packs:\n    publishers: { allow: [acme] }\n");

    let globex = upstream("globex", "");
    let out = yunta_in!(
        &repo,
        &home,
        &["pack", "add", globex.path().to_str().unwrap()]
    );
    assert!(!out.status.success());
    assert_eq!(
        stderr(&out).trim_end(),
        "error: publisher `globex` is not in `permissions.packs.publishers.allow` \
         (declared by the repo config layer) — allowed: acme. Add the publisher there, \
         or install a pack from an allowed publisher.",
        "the refusal names the publisher, the policy field and the declaring layer"
    );
    assert!(
        !repo.join(".yunta/packs/globex").exists(),
        "a refused add must not vendor anything"
    );

    // The allowed publisher installs fine under the same config.
    let acme = upstream("acme", "");
    let ok = yunta_in!(
        &repo,
        &home,
        &["pack", "add", acme.path().to_str().unwrap()]
    );
    assert!(ok.status.success(), "{}", stderr(&ok));
    assert!(stdout(&ok).contains("installed acme/tools-pack"));
}

#[test]
fn executors_deny_refuses_even_with_yes() {
    let (_root, repo, home) = setup_project("permissions:\n  packs:\n    executors: deny\n");

    let source = upstream("acme", "runner.py");
    let out = yunta_in!(
        &repo,
        &home,
        &["pack", "add", source.path().to_str().unwrap(), "--yes"]
    );
    assert!(!out.status.success());
    assert_eq!(
        stderr(&out).trim_end(),
        "error: this pack declares 1 executor(s) and `permissions.packs.executors` is `deny` \
         (declared by the repo config layer) — `--yes` cannot override a permissions ceiling. \
         Change the policy there, or install a pack without executors.",
        "the refusal names the deny policy, the declaring layer and that --yes cannot override it"
    );
    assert!(
        !repo.join(".yunta/packs/acme").exists(),
        "deny must refuse even with --yes"
    );
}

#[test]
fn executors_allow_installs_without_yes() {
    let (_root, repo, home) = setup_project("permissions:\n  packs:\n    executors: allow\n");

    let source = upstream("acme", "runner.py");
    let out = yunta_in!(
        &repo,
        &home,
        &["pack", "add", source.path().to_str().unwrap()]
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("installed acme/tools-pack"));
}

#[test]
fn executors_prompt_still_requires_yes() {
    let (_root, repo, home) = setup_project("permissions:\n  packs:\n    executors: prompt\n");

    let source = upstream("acme", "runner.py");
    let refused = yunta_in!(
        &repo,
        &home,
        &["pack", "add", source.path().to_str().unwrap()]
    );
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("--yes"), "{}", stderr(&refused));

    let ok = yunta_in!(
        &repo,
        &home,
        &["pack", "add", source.path().to_str().unwrap(), "--yes"]
    );
    assert!(ok.status.success(), "{}", stderr(&ok));
}

#[test]
fn update_to_a_ref_that_adds_executors_is_gated_like_add() {
    // v1 is fully declarative — installs with no confirmation under the
    // default (prompt) policy. v2 adds an executor: the update is where
    // the code first appears, so the gate must fire there too, or the
    // policy is theater.
    let (_root, repo, home) = setup_project("");
    let source = tempfile::tempdir().unwrap();
    init_repo(source.path());
    write_pack(source.path(), "acme", "1.0.0", "");

    let add_out = yunta_in!(
        &repo,
        &home,
        &["pack", "add", source.path().to_str().unwrap()]
    );
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    write_pack(source.path(), "acme", "2.0.0", "runner.py");
    let refused = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "acme/tools-pack", INITIAL_BRANCH]
    );
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("--yes"), "{}", stderr(&refused));
    let vendored: serde_norway::Value = serde_norway::from_str(
        &std::fs::read_to_string(repo.join(".yunta/packs/acme/tools-pack/pack.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        vendored["version"], "1.0.0",
        "a refused update must leave the old version vendored"
    );

    let ok = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "acme/tools-pack", INITIAL_BRANCH, "--yes"]
    );
    assert!(ok.status.success(), "{}", stderr(&ok));
    let vendored: serde_norway::Value = serde_norway::from_str(
        &std::fs::read_to_string(repo.join(".yunta/packs/acme/tools-pack/pack.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        vendored["version"], "2.0.0",
        "the confirmed update re-vendors the pack at the new version"
    );
}

#[test]
fn update_refuses_a_publisher_no_longer_allowed() {
    // Installed while unrestricted; the config then narrows the
    // allow-list — the next update must refuse rather than keep pulling
    // content from a publisher the policy no longer trusts.
    let (_root, repo, home) = setup_project("");
    let source = tempfile::tempdir().unwrap();
    init_repo(source.path());
    write_pack(source.path(), "globex", "1.0.0", "");

    let add_out = yunta_in!(
        &repo,
        &home,
        &["pack", "add", source.path().to_str().unwrap()]
    );
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "permissions:\n  packs:\n    publishers: { allow: [acme] }\n",
    )
    .unwrap();
    write_pack(source.path(), "globex", "2.0.0", "");

    let out = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "globex/tools-pack", INITIAL_BRANCH]
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("permissions.packs.publishers.allow"),
        "{}",
        stderr(&out)
    );
    let vendored: serde_norway::Value = serde_norway::from_str(
        &std::fs::read_to_string(repo.join(".yunta/packs/globex/tools-pack/pack.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        vendored["version"], "1.0.0",
        "an update refused on the new allow-list keeps the old version vendored"
    );
}
