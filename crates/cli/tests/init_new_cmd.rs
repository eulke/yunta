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
    let parsed: serde_norway::Value = serde_norway::from_str(&config).unwrap();
    assert_eq!(
        parsed["project"]["name"], "repo",
        "init writes a project section naming the repo: {config}"
    );

    let gitignore = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert!(
        gitignore.lines().any(|l| l == ".yunta/runs/"),
        "init ignores the run-state directory: {gitignore}"
    );

    let out = stdout(&result);
    assert!(
        out.lines().any(|l| l
            == "suggested line for this repo's CLAUDE.md (paste it yourself — Yunta never writes to that file):"),
        "init points at CLAUDE.md without ever writing it: {out}"
    );
    assert!(
        out.lines().any(
            |l| l == "next: run `yunta doctor` to confirm everything above is actually usable."
        ),
        "init directs the user to `yunta doctor` next: {out}"
    );
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
    // What the degradation says is what was missing: a prompt needs a
    // terminal at both ends, and this process was handed neither.
    assert!(
        stderr(&result).contains("no terminal to ask on"),
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

/// A project whose config names a runner every session can use, so a
/// skeleton's agent nodes have somewhere to run.
const RUNNABLE: &str = "defaults:\n  runner: implementer\nrunners:\n  implementer:\n    - { adapter: mock, model: mock-model }\n";

/// [`setup`], with [`RUNNABLE`] as the project's config: what a test about
/// the file `new` writes, rather than about the config, starts from.
fn runnable_setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let (root, repo, home) = setup();
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(repo.join(".yunta/config.yaml"), RUNNABLE).unwrap();
    (root, repo, home)
}

#[test]
fn every_new_shape_writes_a_workflow_that_passes_check() {
    let (_root, repo, home) = runnable_setup();

    for shape in ["one-node", "lint-fix", "tasks"] {
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
    let (_root, repo, home) = runnable_setup();

    for shape in ["one-node", "lint-fix", "tasks"] {
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
    let (_root, repo, home) = runnable_setup();

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
fn new_writes_before_init_ever_ran_and_says_the_workflow_cannot_run_yet() {
    let (_root, repo, home) = setup();
    // The repository holds no `.yunta/config.yaml`. The skeleton names no
    // `runner:`: it is written, and `new` says, as `check` would, that it
    // cannot run until the config declares one.
    let result = yunta_in!(&repo, &home, &["new", "standalone", "--shape", "tasks"]);
    assert!(repo.join(".yunta/workflows/standalone.yaml").is_file());
    assert!(stdout(&result).contains("wrote "), "{}", stdout(&result));
    let said = stderr(&result);
    assert!(
        !result.status.success(),
        "a workflow that cannot run is not OK"
    );
    assert!(
        said.contains("node `plan`") && said.contains("defaults.runner"),
        "got: {said}"
    );
}

#[test]
fn new_and_check_agree_on_a_workflow_that_needs_a_runner() {
    let (_root, repo, home) = setup();
    let new = yunta_in!(&repo, &home, &["new", "lf", "--shape", "lint-fix"]);
    let check = yunta_in!(&repo, &home, &["check", "lf"]);

    assert_eq!(
        new.status.code(),
        check.status.code(),
        "one verdict, one exit code"
    );
    assert!(!check.status.success());
    let problem = |said: &str| {
        said.lines()
            .find(|line| line.contains("node `fix`"))
            .map(str::trim)
            .map(str::to_string)
    };
    assert!(problem(&stderr(&check)).is_some(), "{}", stderr(&check));
    assert_eq!(
        problem(&stderr(&new)),
        problem(&stderr(&check)),
        "`new` says what `check` says, in the same words"
    );
}

#[test]
fn new_lint_fix_runs_the_projects_lint() {
    let (_root, repo, home) = runnable_setup();
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        format!("{RUNNABLE}commands:\n  lint: \"cargo clippy\"\n"),
    )
    .unwrap();

    let result = yunta_in!(&repo, &home, &["new", "lf", "--shape", "lint-fix"]);
    assert!(result.status.success(), "{}", stderr(&result));
    let written = std::fs::read_to_string(repo.join(".yunta/workflows/lf.yaml")).unwrap();
    assert!(
        written.contains("run: { command: lint }"),
        "the lint step runs the command the project declares: {written}"
    );
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
    let (_root, repo, home) = runnable_setup();

    // Every shape the CLI writes parses back as a real `Workflow` — `new`
    // builds the type from its skeleton before the file ever touches disk.
    for shape in ["one-node", "lint-fix", "tasks"] {
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

#[test]
fn init_writes_the_commands_and_suite_a_pnpm_project_declares() {
    let (_root, repo, home) = setup();
    std::fs::write(
        repo.join("package.json"),
        r#"{ "scripts": { "lint": "eslint .", "check-types": "tsc", "test": "vitest run" } }"#,
    )
    .unwrap();
    std::fs::write(repo.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();

    let result = yunta_in!(&repo, &home, &["init"]);
    assert!(result.status.success(), "stderr: {}", stderr(&result));

    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    let parsed: serde_norway::Value = serde_norway::from_str(&config).unwrap();
    assert_eq!(parsed["commands"]["lint"], "pnpm lint", "{config}");
    assert_eq!(
        parsed["commands"]["typecheck"], "pnpm check-types",
        "{config}"
    );
    assert_eq!(parsed["commands"]["test"], "pnpm test", "{config}");
    assert_eq!(parsed["baseline"]["suite"], "pnpm test", "{config}");
    assert!(
        parsed.get("forge").is_none(),
        "no origin, no forge: {config}"
    );
}

#[test]
fn init_writes_the_github_forge_origin_points_at() {
    let (_root, repo, home) = setup();
    yunta_testkit::git(
        &repo,
        &["remote", "add", "origin", "git@github.com:acme/web.git"],
    );

    let result = yunta_in!(&repo, &home, &["init"]);
    assert!(result.status.success(), "stderr: {}", stderr(&result));

    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    let parsed: serde_norway::Value = serde_norway::from_str(&config).unwrap();
    assert_eq!(parsed["forge"]["github"]["repo"], "acme/web", "{config}");
    assert_eq!(
        parsed["forge"]["github"]["token_env"], "GITHUB_TOKEN",
        "{config}"
    );
    assert!(parsed.get("commands").is_none(), "{config}");
}
