//! `yunta init`: prepares a repo for Yunta once — detects ecosystem,
//! test command, base branch and available adapter CLIs (`probe()`,
//! same call `doctor`/`run` use), writes `.yunta/config.yaml` and
//! defensive `.gitignore` entries, installs the mechanism skill, and
//! offers (never writes) a CLAUDE.md line. Non-interactive by default:
//! every step picks the best answer it can detect and reports it;
//! `-i/--interactive` only adds confirmation prompts, and degrades to
//! non-interactive with a warning when stdin isn't a TTY — `init` must
//! never hang waiting for input that isn't coming.

use std::path::Path;

use yunta_core::port::ProbeReport;
use yunta_core::{AdapterId, AdapterSettings};
use yunta_engine::process::Supervision;

use crate::ask::{ask_line, Console, Escape};
use crate::error::{warn, CliError, Outcome};
use crate::interrupt::Interrupt;
use crate::surface::Diagnostics;

const MECHANISM_SKILL_DIR: &str = ".yunta/skills/yunta-mechanism";

struct Ecosystem {
    name: &'static str,
    test_cmd: &'static str,
    /// A tip printed to the terminal, never written to config — no key
    /// for this exists anywhere in the reference schema; this is a
    /// deliberate scoping decision, not an oversight.
    cache_tip: &'static str,
}

/// First match wins, in the order listed — a repo with both `Cargo.toml`
/// and `package.json` (a Rust project with a small JS tool inside) is
/// still primarily a Rust project for this purpose.
fn detect_ecosystem(repo: &Path) -> Option<Ecosystem> {
    let candidates = [
        (
            "Cargo.toml",
            Ecosystem {
                name: "rust",
                test_cmd: "cargo test",
                cache_tip: "share a build cache across worktrees: export \
                            CARGO_TARGET_DIR=$HOME/.cache/yunta-cargo-target \
                            before running yunta — otherwise every \
                            worktree rebuilds the whole dependency tree.",
            },
        ),
        (
            "package.json",
            Ecosystem {
                name: "node",
                test_cmd: "npm test",
                cache_tip: "share a package cache across worktrees: point \
                            npm's cache at a shared directory (`npm config \
                            set cache <shared-dir>`) or use a package manager \
                            with content-addressed storage.",
            },
        ),
        (
            "go.mod",
            Ecosystem {
                name: "go",
                test_cmd: "go test ./...",
                cache_tip: "Go's own build/module caches (GOCACHE/GOMODCACHE) \
                            are already shared machine-wide by default — \
                            nothing extra to configure for worktrees.",
            },
        ),
        (
            "pyproject.toml",
            Ecosystem {
                name: "python",
                test_cmd: "pytest",
                cache_tip: "share a virtualenv or package cache across \
                            worktrees (e.g. a shared `uv`/`pip` cache dir) to \
                            avoid reinstalling dependencies per worktree.",
            },
        ),
    ];
    candidates
        .into_iter()
        .find(|(marker, _)| repo.join(marker).is_file())
        .map(|(_, ecosystem)| ecosystem)
}

async fn detect_base_branch(repo: &Path, supervision: Supervision<'_>) -> String {
    // Both probes are best-effort: a git that can't answer (no remote
    // HEAD, detached head, no repo) falls through to the next, then to
    // the conventional default.
    if let Ok(raw) = yunta_engine::git::output(
        repo,
        &["symbolic-ref", "refs/remotes/origin/HEAD"],
        supervision,
    )
    .await
    {
        if let Some(branch) = raw.trim().strip_prefix("refs/remotes/origin/") {
            if !branch.is_empty() {
                return branch.to_string();
            }
        }
    }
    if let Ok(name) =
        yunta_engine::git::output(repo, &["branch", "--show-current"], supervision).await
    {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    "main".to_string()
}

struct ProbedAdapter {
    id: AdapterId,
    healthy: bool,
    detail: String,
}

/// Probes every adapter this binary builds, in the order the
/// composition root declares them — the id each reports about itself,
/// never one re-spelled here.
async fn probe_known_adapters() -> Vec<ProbedAdapter> {
    let mut probed = Vec::new();
    for adapter in super::built_adapters(|_| AdapterSettings::default()) {
        let id = adapter.id().clone();
        probed.push(match adapter.probe().await {
            Ok(ProbeReport::Healthy { version }) => ProbedAdapter {
                id,
                healthy: true,
                detail: version.unwrap_or_else(|| "version unknown".to_string()),
            },
            Ok(ProbeReport::Unhealthy { diagnostic }) => ProbedAdapter {
                id,
                healthy: false,
                detail: diagnostic,
            },
            Err(e) => ProbedAdapter {
                id,
                healthy: false,
                detail: e.to_string(),
            },
        });
    }
    probed
}

fn render_config_yaml(project_name: &str, base_branch: &str, probed: &[ProbedAdapter]) -> String {
    let mut out = String::new();
    out.push_str("# Written by `yunta init` — team-shared, commit this file.\n");
    out.push_str("# Personal overrides belong in ~/.yunta/config.yaml (the user\n");
    out.push_str("# config layer) — never in this file.\n\n");
    out.push_str("project:\n");
    out.push_str(&format!("  name: {project_name}\n"));
    out.push_str(&format!("  base_branch: {base_branch}\n"));
    out.push_str("  branch_prefix: yunta/\n\n");

    out.push_str("# `runners:` names roles your workflows' `runner:` fields reference,\n");
    out.push_str("# each with one or more adapter candidates (first capable one wins).\n");
    for adapter in probed {
        if adapter.healthy {
            out.push_str(&format!(
                "# detected: {} ({})\n",
                adapter.id, adapter.detail
            ));
        } else {
            out.push_str(&format!(
                "# not available: {} ({})\n",
                adapter.id, adapter.detail
            ));
        }
    }
    out.push_str("# runners:\n");
    out.push_str("#   implementer:\n");
    if let Some(healthy) = probed.iter().find(|a| a.healthy) {
        out.push_str(&format!(
            "#     - {{ adapter: {}, model: <model-name> }}\n",
            healthy.id
        ));
    } else {
        out.push_str(&format!(
            "#     - {{ adapter: {}, model: <model-name> }}\n",
            super::first_built_adapter()
        ));
    }
    out
}

const GITIGNORE_MARKER: &str = "# added by `yunta init`";

/// Defensive only — `paths.runs`/`paths.worktrees` default to `~/.yunta`,
/// outside the repo entirely, so none of this exists in a repo with
/// default config. It's here for the day someone points `paths:` back
/// into the repo on purpose.
fn gitignore_block() -> String {
    format!(
        "\n{GITIGNORE_MARKER} — only matters if `paths:` ever points inside \
         this repo (defaults keep run state in ~/.yunta):\n.yunta/runs/\n\
         .yunta/worktrees/\n.yunta/*.db\n.yunta/*.db-*\n"
    )
}

fn write_gitignore(repo: &Path) -> std::io::Result<bool> {
    let path = repo.join(".gitignore");
    // An absent `.gitignore` is the create case; any *other* read error
    // (unreadable, not UTF-8, a directory) is propagated, never swallowed
    // as "empty" — appending to an empty string would then overwrite a
    // file whose real content we failed to read.
    let existing = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if existing.contains(GITIGNORE_MARKER) {
        return Ok(false);
    }
    let mut updated = existing;
    updated.push_str(&gitignore_block());
    std::fs::write(&path, updated)?;
    Ok(true)
}

/// The mechanism skill: teaches a client agent when to prefer a
/// verified workflow over ad-hoc implementation and how to discover
/// what exists (`yunta list` / `list_workflows`) — the skill itself
/// never embeds a catalog, since the catalog is consulted at the
/// moment it's needed rather than baked in, and one written at `init`
/// time would be empty and stale the moment anyone adds a workflow.
/// Format: a `SKILL.md` with frontmatter, the shape Claude Code's own
/// skill mechanism reads — no other file format is specified anywhere
/// in the docs this was built against.
fn mechanism_skill_content() -> String {
    "---\n\
     name: yunta-mechanism\n\
     description: When and how to use Yunta's verified workflows instead of ad-hoc implementation.\n\
     ---\n\n\
     # Using Yunta\n\n\
     This repo uses Yunta, a deterministic workflow engine: a workflow is a \
     declarative DAG whose nodes are mechanically verified (criteria, scope, \
     baseline) before anything is considered done, with a full audit log.\n\n\
     Before implementing something ad hoc, check whether a Yunta workflow \
     already covers it — the catalog is never fixed at install time, so \
     always look it up fresh:\n\n\
     - From a shell: `yunta list` (catalog) or `yunta list --runs` (local \
       run state).\n\
     - From an MCP-connected client: the `list_workflows` tool.\n\n\
     If a workflow covers the task, prefer `yunta run <workflow>` (or the \
     `run_workflow` MCP tool) over reimplementing it by hand — that path \
     gets mechanical verification and an audit trail this one doesn't. If \
     none covers it, proceed as you normally would; nothing here requires \
     using Yunta for work it has no workflow for.\n\n\
     ## Writing a document Yunta reads\n\n\
     A tasks document, a findings artifact and a questions artifact are \
     validated strictly: a key that is not in the shape fails the node \
     that produced it. Never guess the format — ask for it:\n\n\
     - From a shell: `yunta schema <kind>`, or `yunta schema` to list the \
       kinds.\n\
     - From an MCP-connected client: the `document_shape` tool.\n"
        .to_string()
}

fn write_mechanism_skill(repo: &Path, force: bool) -> std::io::Result<bool> {
    let dir = repo.join(MECHANISM_SKILL_DIR);
    let path = dir.join("SKILL.md");
    if path.exists() && !force {
        return Ok(false);
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, mechanism_skill_content())?;
    Ok(true)
}

fn claude_md_suggestion() -> &'static str {
    "  This repo uses Yunta for verified workflows — before implementing\n  \
     something ad hoc, check `yunta list` (or the `list_workflows` MCP\n  \
     tool) for an existing verified workflow that already covers it."
}

/// One setting asked for on `console`, or `default` when nobody
/// answers.
///
/// Answered on the one line every prompt in this binary is answered on
/// — same editing, same Escape, same Ctrl-C, same terminal handed back
/// — so a person who has answered a run answers `init` the same way.
/// An empty line takes the default, and so does Escape: "not me, not
/// now" about a setting that already has a detected value is that
/// value.
fn asked(console: &Console, prompt: &str, default: &str) -> String {
    match ask_line(console, &format!("{prompt} [{default}]: ")) {
        Ok(typed) if !typed.value.is_empty() => typed.value,
        _ => default.to_string(),
    }
}

pub async fn init(interactive: bool, force: bool) -> Result<Outcome, CliError> {
    let repo = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    // `init` is the command that makes a project, so there is no
    // `Context` to resolve yet — and its probes still spawn git, so it
    // owns the interruption itself, like any other invocation.
    let interrupt = Interrupt::ctrl_c()
        .map_err(|source| CliError::io("install the interrupt handler for", "init", source))?;
    let clock = yunta_core::SystemClock;
    let supervision = Supervision::outside_any_run(interrupt.stop(), &clock);

    let config_path = repo.join(".yunta/config.yaml");
    if config_path.exists() && !force {
        return Err(CliError::msg(format!(
            "{} already exists — pass --force to overwrite",
            config_path.display()
        )));
    }

    // `-i` degrades with a warning rather than hanging on a terminal
    // that will never produce a line. There is no run drawing here, so
    // what opening the console has to say goes out through a door onto
    // nothing, which is stderr.
    let console = match interactive {
        true => Console::open(&Diagnostics::none(), Escape::KeepsDefault).await,
        false => None,
    };
    if interactive && console.is_none() {
        warn("--interactive given but there is no terminal to ask on — using detected defaults");
    }

    let default_name = repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workflow-project".to_string());
    let default_branch = detect_base_branch(&repo, supervision).await;
    let ecosystem = detect_ecosystem(&repo);

    let (project_name, base_branch) = match &console {
        Some(console) => {
            // What Escape does, said once above the prompts it applies
            // to — the same place every other surface says it.
            let _ = console.say(console.escape().said());
            (
                asked(console, "project name", &default_name),
                asked(console, "base branch", &default_branch),
            )
        }
        None => (default_name, default_branch),
    };

    let probed = probe_known_adapters().await;

    let mut wrote = Vec::new();
    let mut skipped = Vec::new();

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| CliError::io("create", parent.display(), source))?;
    }
    let config_yaml = render_config_yaml(&project_name, &base_branch, &probed);
    // Never write a config this binary can't read back: parse the
    // generated text into a real `ConfigLayer` first, so a broken template
    // fails here instead of leaving an unreadable file — the more so under
    // `--force`, which would otherwise overwrite a good config with a bad
    // one.
    yunta_core::yaml::parse::<yunta_core::ConfigLayer>(&config_yaml)
        .map_err(|e| CliError::msg(format!("the generated config is not valid: {e}")))?;
    std::fs::write(&config_path, config_yaml)
        .map_err(|source| CliError::io("write", config_path.display(), source))?;
    wrote.push(config_path.display().to_string());

    match write_gitignore(&repo) {
        Ok(true) => wrote.push(repo.join(".gitignore").display().to_string()),
        Ok(false) => skipped.push(".gitignore (already has yunta entries)".to_string()),
        Err(source) => return Err(CliError::io("update", ".gitignore", source)),
    }

    match write_mechanism_skill(&repo, force) {
        Ok(true) => wrote.push(format!("{MECHANISM_SKILL_DIR}/SKILL.md")),
        Ok(false) => skipped.push(format!(
            "{MECHANISM_SKILL_DIR}/SKILL.md (already exists, pass --force to rewrite)"
        )),
        Err(source) => return Err(CliError::io("write", "the mechanism skill", source)),
    }

    println!("yunta init: done in {}", repo.display());
    for path in &wrote {
        println!("  wrote {path}");
    }
    for path in &skipped {
        println!("  skipped {path}");
    }

    match &ecosystem {
        Some(eco) => println!(
            "\ndetected ecosystem: {} (suggested test command: `{}`)\ntip: {}",
            eco.name, eco.test_cmd, eco.cache_tip
        ),
        None => println!(
            "\nno known ecosystem detected (looked for Cargo.toml, package.json, \
             go.mod, pyproject.toml) — fill in a test command by hand"
        ),
    }

    for adapter in &probed {
        let status = if adapter.healthy {
            "healthy"
        } else {
            "unavailable"
        };
        println!("adapter {}: {status} ({})", adapter.id, adapter.detail);
    }

    println!(
        "\nsuggested line for this repo's CLAUDE.md (paste it yourself — \
         Yunta never writes to that file):\n\n{}\n",
        claude_md_suggestion()
    );

    println!("next: run `yunta doctor` to confirm everything above is actually usable.");

    Ok(Outcome::Success)
}
