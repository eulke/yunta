//! Packs installed for every project, through the real compiled binary: `--global`
//! puts a pack in the user's store (the state root), the pack commands act on that
//! store with it, and a project that installs no pack of its own reads the user's,
//! held to the ceiling its own manifest declares.

use std::path::{Path, PathBuf};

use yunta_testkit::{git, init_repo, stderr, stdout, unwrapped, yunta_in};

/// A pack repository at `dir`, `acme/review-pack` at `version`, whose one workflow,
/// `review`, runs `node`: a bash node needs no adapter, so it runs anywhere.
fn write_pack(dir: &Path, version: &str, permissions: &str, node: &str) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        format!(
            "name: review-pack\npublisher: acme\nversion: {version}\ndescription: a pack\n\
             declares:\n  permissions: {permissions}\n  network: false\n  executors: []\n\
             contents:\n  workflows: [workflows/review.yaml]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        format!("name: review\ndescription: a review every project runs\nnodes:\n{node}"),
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", version]);
}

/// A node that runs a shell command and asks no adapter.
const BASH: &str = "  - id: noop\n    kind: bash\n    run: \"true\"\n";

/// A prompt node that asks to edit files.
const EDITS: &str = "  - id: draft\n    kind: prompt\n    prompt: hi\n    permissions: edit\n";

/// A temporary root holding the pack's repository at `1.0.0`, tagged, a project that
/// installs no pack, and the state root both read: `(root, upstream, repo, home)`.
fn setup(permissions: &str, node: &str) -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack(&upstream, "1.0.0", permissions, node);
    git(&upstream, &["tag", "v1.0.0"]);
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    (root, upstream, repo, home)
}

/// `pack add --global` of `upstream`, from `repo`, succeeded.
fn added_for_every_project(upstream: &Path, repo: &Path, home: &Path) {
    let source = upstream.to_str().unwrap();
    let added = yunta_in!(repo, home, &["pack", "add", "--global", source]);
    assert!(added.status.success(), "{}", stderr(&added));
}

#[test]
fn an_install_for_every_project_lands_in_the_state_root_and_leaves_the_project_untouched() {
    let (_root, upstream, repo, home) = setup("read-only", BASH);

    added_for_every_project(&upstream, &repo, &home);

    assert!(home.join("packs/acme/review-pack/pack.yaml").is_file());
    assert!(home.join("yunta.lock").is_file());
    assert!(!repo.join(".yunta/packs").exists());
    assert!(!repo.join(".yunta/yunta.lock").exists());
    let global = stdout(&yunta_in!(&repo, &home, &["pack", "list", "--global"]));
    assert!(global.contains("acme/review-pack"), "{global}");
    let project = stdout(&yunta_in!(&repo, &home, &["pack", "list"]));
    assert!(!project.contains("acme/review-pack"), "{project}");
}

#[test]
fn audit_update_and_remove_with_global_act_on_the_users_pack() {
    let (_root, upstream, repo, home) = setup("read-only", BASH);
    added_for_every_project(&upstream, &repo, &home);

    let audited = yunta_in!(
        &repo,
        &home,
        &["pack", "audit", "--global", "acme/review-pack"]
    );
    assert!(audited.status.success(), "{}", stderr(&audited));
    assert!(stdout(&audited).contains("noop"), "{}", stdout(&audited));

    write_pack(&upstream, "2.0.0", "read-only", BASH);
    git(&upstream, &["tag", "v2.0.0"]);
    let updated = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "--global", "acme/review-pack", "v2.0.0"]
    );
    assert!(updated.status.success(), "{}", stderr(&updated));
    let manifest = std::fs::read_to_string(home.join("packs/acme/review-pack/pack.yaml")).unwrap();
    assert!(manifest.contains("version: 2.0.0"), "{manifest}");
    let lock = std::fs::read_to_string(home.join("yunta.lock")).unwrap();
    assert!(lock.contains("v2.0.0"), "{lock}");

    let from_the_project = yunta_in!(&repo, &home, &["pack", "remove", "acme/review-pack"]);
    assert!(!from_the_project.status.success());
    assert!(home.join("packs/acme/review-pack").exists());

    let removed = yunta_in!(
        &repo,
        &home,
        &["pack", "remove", "--global", "acme/review-pack"]
    );
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(!home.join("packs/acme/review-pack").exists());
    let lock = std::fs::read_to_string(home.join("yunta.lock")).unwrap();
    assert!(!lock.contains("acme/review-pack"), "{lock}");
}

#[test]
fn a_project_with_no_pack_of_its_own_checks_runs_and_lists_the_users_pack() {
    let (_root, upstream, repo, home) = setup("read-only", BASH);
    added_for_every_project(&upstream, &repo, &home);

    let checked = yunta_in!(&repo, &home, &["check", "acme/review"]);
    assert!(checked.status.success(), "{}", stderr(&checked));
    let ran = yunta_in!(&repo, &home, &["run", "acme/review"]);
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert!(stdout(&ran).contains("finished"), "{}", stdout(&ran));
    let listed = stdout(&yunta_in!(&repo, &home, &["list"]));
    assert!(
        listed
            .lines()
            .any(|line| line.trim() == "acme/review: a review every project runs"),
        "{listed}"
    );
}

#[test]
fn check_refuses_a_users_pack_whose_node_passes_its_declared_ceiling() {
    let (_root, upstream, repo, home) = setup("read-only", EDITS);
    added_for_every_project(&upstream, &repo, &home);

    let checked = yunta_in!(&repo, &home, &["check", "acme/review"]);

    assert!(!checked.status.success());
    let said = unwrapped(&stderr(&checked));
    assert!(
        said.contains(
            "node `draft` requests permissions `edit` but pack `acme/review-pack` declares a \
             ceiling of `read-only`"
        ),
        "{said}"
    );
}

#[test]
fn a_refusal_about_a_users_pack_suggests_the_command_with_global() {
    let (_root, upstream, repo, home) = setup("read-only", BASH);
    added_for_every_project(&upstream, &repo, &home);

    let source = upstream.to_str().unwrap();
    let again = yunta_in!(&repo, &home, &["pack", "add", "--global", source]);
    assert!(!again.status.success());
    let said = unwrapped(&stderr(&again));
    assert!(
        said.contains("`yunta pack update --global acme/review-pack <ref>`")
            && said.contains("`yunta pack remove --global acme/review-pack`"),
        "{said}"
    );

    let unknown = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "--global", "acme/other-pack", "v1.0.0"]
    );
    assert!(!unknown.status.success());
    let said = unwrapped(&stderr(&unknown));
    assert!(
        said.contains("`yunta pack add --global <source>` it first"),
        "{said}"
    );
}

#[test]
fn a_command_aimed_at_the_store_that_lacks_a_pack_names_the_store_that_holds_it() {
    let (_root, upstream, repo, home) = setup("read-only", BASH);
    added_for_every_project(&upstream, &repo, &home);

    // The pack is among the user's packs; each command is aimed at the project.
    let removed = yunta_in!(&repo, &home, &["pack", "remove", "acme/review-pack"]);
    assert!(!removed.status.success());
    let said = unwrapped(&stderr(&removed));
    assert!(
        said.contains("yunta pack remove --global acme/review-pack"),
        "{said}"
    );
    assert!(
        home.join("packs/acme/review-pack/pack.yaml").is_file(),
        "a refused remove leaves the user's pack where it is"
    );

    let updated = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "acme/review-pack", "v1.0.0"]
    );
    assert!(!updated.status.success());
    let said = unwrapped(&stderr(&updated));
    assert!(
        said.contains("yunta pack update --global acme/review-pack"),
        "{said}"
    );
    assert!(
        !said.contains("yunta pack add"),
        "a second install would shadow the user's pack: {said}"
    );

    let audited = yunta_in!(&repo, &home, &["pack", "audit", "acme/review-pack"]);
    assert!(!audited.status.success());
    let said = unwrapped(&stderr(&audited));
    assert!(
        said.contains("yunta pack audit --global acme/review-pack"),
        "{said}"
    );
}

#[test]
fn a_global_command_about_a_pack_only_the_project_holds_names_the_project() {
    let (_root, upstream, repo, home) = setup("read-only", BASH);
    let source = upstream.to_str().unwrap();
    let added = yunta_in!(&repo, &home, &["pack", "add", source]);
    assert!(added.status.success(), "{}", stderr(&added));

    let removed = yunta_in!(
        &repo,
        &home,
        &["pack", "remove", "--global", "acme/review-pack"]
    );
    assert!(!removed.status.success());
    let said = unwrapped(&stderr(&removed));
    assert!(
        said.contains("`yunta pack remove acme/review-pack`"),
        "{said}"
    );
    assert!(repo
        .join(".yunta/packs/acme/review-pack/pack.yaml")
        .is_file());
}
