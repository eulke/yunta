//! A key a workflow needs and the config lacks is named with what this
//! repository answers for it — by `check`, by `doctor`, and by `pack
//! add` as soon as the pack lands — and none of them refuses more for
//! it than it already did.

use std::path::{Path, PathBuf};

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in};

/// A pnpm project that declares a linter and a type check.
fn pnpm_repo(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    std::fs::write(
        repo.join("package.json"),
        r#"{ "scripts": { "lint": "eslint .", "typecheck": "tsc --noEmit" } }"#,
    )
    .unwrap();
    std::fs::write(repo.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
    repo
}

/// Lints, then type-checks where the project says how.
const LINTS: &str = "name: lints\nnodes:\n  \
    - { id: lint, kind: bash, run: { command: lint } }\n  \
    - { id: types, kind: bash, depends_on: [lint], optional: true, run: { command: typecheck } }\n";

/// A pack whose one workflow is [`LINTS`].
fn lint_pack(root: &Path) -> PathBuf {
    let upstream = root.join("upstream");
    std::fs::create_dir_all(upstream.join("workflows")).unwrap();
    init_repo(&upstream);
    std::fs::write(
        upstream.join("pack.yaml"),
        "name: lint-pack\npublisher: acme\nversion: 1.0.0\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/lints.yaml]\n",
    )
    .unwrap();
    std::fs::write(upstream.join("workflows/lints.yaml"), LINTS).unwrap();
    git(&upstream, &["add", "."]);
    git(&upstream, &["commit", "-q", "-m", "v1"]);
    upstream
}

#[test]
fn check_suggests_the_command_this_repository_runs() {
    let root = tempfile::tempdir().unwrap();
    let repo = pnpm_repo(root.path());
    std::fs::write(repo.join("lints.yaml"), LINTS).unwrap();

    let out = yunta_in!(&repo, &root.path().join("home"), &["check", "lints.yaml"]);

    assert!(!out.status.success(), "a required command is still refused");
    let said = stderr(&out);
    assert!(
        said.lines()
            .any(|l| l.trim() == "detected here: declare `commands: { lint: \"pnpm lint\" }`"),
        "{said}"
    );
    assert!(
        !said.contains("typecheck"),
        "an optional node is left out, not refused: {said}"
    );
}

#[test]
fn pack_add_reports_what_the_pack_needs_with_what_it_detected() {
    let root = tempfile::tempdir().unwrap();
    let repo = pnpm_repo(root.path());
    let upstream = lint_pack(root.path());

    let out = yunta_in!(
        &repo,
        &root.path().join("home"),
        &["pack", "add", upstream.to_str().unwrap()]
    );

    assert!(out.status.success(), "{}", stderr(&out));
    let said = stdout(&out);
    let needs: Vec<&str> = said
        .lines()
        .skip_while(|l| *l != "acme/lint-pack needs from this project:")
        .take(5)
        .collect();
    assert_eq!(
        needs,
        [
            "acme/lint-pack needs from this project:",
            "  node `lint`: the project declares no command `lint` — a run is refused until it does",
            "    detected here: declare `commands: { lint: \"pnpm lint\" }`",
            "  node `types`: the project declares no command `typecheck` — a run leaves the node out until it does",
            "    detected here: declare `commands: { typecheck: \"pnpm typecheck\" }`",
        ],
        "{said}"
    );
}

#[test]
fn doctor_names_what_a_pack_needs_with_the_detected_value() {
    let root = tempfile::tempdir().unwrap();
    let repo = pnpm_repo(root.path());
    let upstream = lint_pack(root.path());
    let home = root.path().join("home");
    let added = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(added.status.success(), "{}", stderr(&added));

    let out = yunta_in!(&repo, &home, &["doctor"]);

    assert!(!out.status.success());
    let said = stdout(&out);
    assert!(
        said.lines().any(|l| l
            == "pack acme/lint-pack: detected here: declare `commands: { lint: \"pnpm lint\" }`"),
        "{said}"
    );
}
