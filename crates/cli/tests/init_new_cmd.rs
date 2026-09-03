//! `yunta init`/`yunta new` end-to-end: both must work
//! without a TTY in a clean container and be idempotent (refuse to
//! clobber existing state without `--force`); every `new`-generated
//! workflow must pass `check`; `new` must never reference a pack or
//! touch `yunta.lock`.

use yunta_testkit::{init_repo, stderr, stdout, yunta_in};

fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    (root, repo, home)
}

#[test]
fn init_writes_config_gitignore_and_the_mechanism_skill() {
    let (_root, repo, home) = setup();

    let result = yunta_in!(&repo, &home, &["init"]);
    assert!(result.status.success(), "stderr: {}", stderr(&result));

    assert!(repo.join(".yunta/config.yaml").is_file());
    assert!(repo.join(".gitignore").is_file());
    assert!(repo
        .join(".yunta/skills/yunta-mechanism/SKILL.md")
        .is_file());

    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    assert!(config.contains("project:"), "got: {config}");

    let gitignore = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert!(gitignore.contains(".yunta/runs/"), "got: {gitignore}");

    let out = stdout(&result);
    assert!(out.contains("CLAUDE.md"), "got: {out}");
    assert!(out.contains("doctor"), "got: {out}");
}

#[test]
fn init_never_writes_to_claude_md_even_when_one_exists() {
    let (_root, repo, home) = setup();
    std::fs::write(repo.join("CLAUDE.md"), "# original\n").unwrap();

    let result = yunta_in!(&repo, &home, &["init"]);
    assert!(result.status.success());

    let claude_md = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
    assert_eq!(claude_md, "# original\n", "init must never touch CLAUDE.md");
}

#[test]
fn init_refuses_to_overwrite_without_force_and_succeeds_with_it() {
    let (_root, repo, home) = setup();

    let first = yunta_in!(&repo, &home, &["init"]);
    assert!(first.status.success());

    let second = yunta_in!(&repo, &home, &["init"]);
    assert!(!second.status.success());
    assert!(
        stderr(&second).contains("--force"),
        "got: {}",
        stderr(&second)
    );

    let forced = yunta_in!(&repo, &home, &["init", "--force"]);
    assert!(forced.status.success(), "stderr: {}", stderr(&forced));
}

#[test]
fn init_interactive_without_a_tty_degrades_instead_of_hanging() {
    let (_root, repo, home) = setup();

    let result = yunta_in!(&repo, &home, &["init", "--interactive"]);
    assert!(result.status.success(), "stderr: {}", stderr(&result));
    assert!(
        stderr(&result).contains("non-interactive") || stderr(&result).contains("TTY"),
        "expected a degrade warning, got: {}",
        stderr(&result)
    );
}

#[test]
fn init_detects_a_rust_ecosystem_from_cargo_toml() {
    let (_root, repo, home) = setup();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();

    let result = yunta_in!(&repo, &home, &["init"]);
    assert!(result.status.success());
    assert!(stdout(&result).contains("rust"), "got: {}", stdout(&result));
}

#[test]
fn every_new_shape_writes_a_workflow_that_passes_check() {
    let (_root, repo, home) = setup();

    for shape in ["one-node", "lint-fix", "ledger"] {
        let name = format!("wf-{shape}");
        let result = yunta_in!(&repo, &home, &["new", &name, "--shape", shape]);
        assert!(
            result.status.success(),
            "shape {shape} failed — stdout: {}\nstderr: {}",
            stdout(&result),
            stderr(&result)
        );
        assert!(stdout(&result).contains("OK"), "got: {}", stdout(&result));
        let path = repo.join(".yunta/workflows").join(format!("{name}.yaml"));
        assert!(path.is_file());
    }
}

#[test]
fn new_never_references_a_pack_or_touches_the_lock_file() {
    let (_root, repo, home) = setup();

    for shape in ["one-node", "lint-fix", "ledger"] {
        let name = format!("structural-{shape}");
        let result = yunta_in!(&repo, &home, &["new", &name, "--shape", shape]);
        assert!(result.status.success());
        let yaml =
            std::fs::read_to_string(repo.join(".yunta/workflows").join(format!("{name}.yaml")))
                .unwrap();
        assert!(
            !yaml.contains("pack"),
            "shape {shape} mentions `pack`: {yaml}"
        );
    }
    assert!(
        !repo.join("yunta.lock").exists(),
        "`new` must never create yunta.lock"
    );
}

#[test]
fn new_rejects_an_unknown_shape() {
    let (_root, repo, home) = setup();
    let result = yunta_in!(&repo, &home, &["new", "bad", "--shape", "nonexistent"]);
    assert!(!result.status.success());
    assert!(
        !repo.join(".yunta/workflows/bad.yaml").exists(),
        "a rejected shape must not write a file"
    );
}

#[test]
fn new_refuses_to_overwrite_without_force_and_succeeds_with_it() {
    let (_root, repo, home) = setup();

    let first = yunta_in!(&repo, &home, &["new", "dup", "--shape", "one-node"]);
    assert!(first.status.success());

    let second = yunta_in!(&repo, &home, &["new", "dup", "--shape", "lint-fix"]);
    assert!(!second.status.success());
    assert!(
        stderr(&second).contains("--force"),
        "got: {}",
        stderr(&second)
    );

    let forced = yunta_in!(
        &repo,
        &home,
        &["new", "dup", "--shape", "lint-fix", "--force"]
    );
    assert!(forced.status.success(), "stderr: {}", stderr(&forced));
    let yaml = std::fs::read_to_string(repo.join(".yunta/workflows/dup.yaml")).unwrap();
    assert!(
        yaml.contains("lint"),
        "expected the lint-fix shape after --force: {yaml}"
    );
}

#[test]
fn new_interactive_without_a_tty_defaults_to_one_node_instead_of_hanging() {
    let (_root, repo, home) = setup();
    let result = yunta_in!(&repo, &home, &["new", "picked", "--interactive"]);
    assert!(result.status.success(), "stderr: {}", stderr(&result));
    let yaml = std::fs::read_to_string(repo.join(".yunta/workflows/picked.yaml")).unwrap();
    assert!(
        yaml.contains("kind: bash"),
        "expected one-node default: {yaml}"
    );
}

#[test]
fn new_rejects_an_unsafe_workflow_name() {
    let (_root, repo, home) = setup();
    let result = yunta_in!(&repo, &home, &["new", "../escape", "--shape", "one-node"]);
    assert!(!result.status.success());
}

#[test]
fn new_works_before_init_ever_ran() {
    let (_root, repo, home) = setup();
    // No `.yunta/config.yaml` exists yet — `check` must still pass, since
    // these skeletons never reference a `runner:` a missing config could
    // fail to resolve.
    let result = yunta_in!(&repo, &home, &["new", "standalone", "--shape", "ledger"]);
    assert!(result.status.success(), "stderr: {}", stderr(&result));
}

#[test]
fn init_never_replaces_an_unreadable_gitignore() {
    let (_root, repo, home) = setup();

    // A `.gitignore` that isn't valid UTF-8: reading it fails with a
    // non-NotFound error, which `init` must propagate rather than treat as
    // "empty" and overwrite — losing whatever the file actually held.
    let gitignore = repo.join(".gitignore");
    std::fs::write(&gitignore, [0xff, 0xfe, 0x00, 0x01, 0x02]).unwrap();
    let before = std::fs::read(&gitignore).unwrap();

    let result = yunta_in!(&repo, &home, &["init"]);
    assert!(
        !result.status.success(),
        "init must fail on an unreadable .gitignore, never clobber it: {}",
        stdout(&result)
    );
    assert!(
        stderr(&result).contains(".gitignore"),
        "the error names the file it could not read: {}",
        stderr(&result)
    );

    let after = std::fs::read(&gitignore).unwrap();
    assert_eq!(
        before, after,
        "the unreadable .gitignore must be left exactly as it was"
    );
}

#[test]
fn new_writes_only_a_parseable_skeleton() {
    let (_root, repo, home) = setup();

    // Every shape the CLI writes parses back as a real `Workflow` — `new`
    // builds the type from its skeleton before the file ever touches disk.
    for shape in ["one-node", "lint-fix", "ledger"] {
        let name = format!("parseable-{shape}");
        let result = yunta_in!(&repo, &home, &["new", &name, "--shape", shape]);
        assert!(
            result.status.success(),
            "shape {shape}: {}",
            stderr(&result)
        );
        let yaml =
            std::fs::read_to_string(repo.join(".yunta/workflows").join(format!("{name}.yaml")))
                .unwrap();
        yunta_core::yaml::parse::<yunta_core::Workflow>(&yaml)
            .unwrap_or_else(|e| panic!("shape {shape} wrote an unparseable workflow: {e}\n{yaml}"));
    }
}
