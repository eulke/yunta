//! `yunta init`: prepares a repo for Yunta once — detects ecosystem,
//! test command, base branch and available adapter CLIs (`probe()`,
//! same call `doctor`/`run` use), writes `.yunta/config.yaml` and
//! defensive `.gitignore` entries, installs the mechanism skill, and
//! offers (never writes) a CLAUDE.md line. Non-interactive by default:
//! every step picks the best answer it can detect and reports it;
//! `-i/--interactive` only adds confirmation prompts, and degrades to
//! non-interactive with a warning when stdin isn't a TTY — `init` must
//! never hang waiting for input that isn't coming.

mod skill;

use skill::MECHANISM_SKILL_DIR;
use std::path::Path;

use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_core::step::NextStep;
use yunta_core::{Detected, ProbedAdapter};
use yunta_engine::process::Supervision;
use yunta_render::command;
use yunta_render::page::Initialized;

use crate::ask::{ask_line, Console, Escape};
use crate::error::CliError;
use crate::output::Outcome;
use crate::shell::Shell;
use crate::surface::Diagnostics;

/// The base branch a project starts from: the remote's default, else the
/// branch checked out here, else `main`. Only git's "no remote HEAD" answer
/// falls through to the next probe. Any other failure stops `init`, since a
/// base read from a git that never answered would be written as if found.
async fn detect_base_branch(repo: &Path, supervision: Supervision<'_>) -> Result<String, CliError> {
    // `-q` makes git exit 1, silently, when there is no remote HEAD to read.
    let remote = yunta_engine::git::read(
        repo,
        &["symbolic-ref", "-q", "refs/remotes/origin/HEAD"],
        supervision,
    )
    .await;
    match remote {
        Ok(raw) => {
            if let Some(branch) = raw.trim().strip_prefix("refs/remotes/origin/") {
                if !branch.is_empty() {
                    return Ok(branch.to_string());
                }
            }
        }
        Err(error)
            if matches!(
                error.cause,
                yunta_engine::git::Cause::Exit { code: Some(1), .. }
            ) => {}
        Err(error) => return Err(yunta_engine::RunError::from(error).into()),
    }
    let current = yunta_engine::git::read(repo, &["branch", "--show-current"], supervision)
        .await
        .map_err(|error| CliError::from(yunta_engine::RunError::from(error)))?;
    let name = current.trim();
    if name.is_empty() {
        return Ok("main".to_string());
    }
    Ok(name.to_string())
}

fn render_config_yaml(
    project_name: &str,
    base_branch: &str,
    probed: &[ProbedAdapter],
    detected: &Detected,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Written by `{}`: team-shared, commit this file.\n",
        command::line(&NextStep::Init)
    ));
    out.push_str("# Personal overrides belong in ~/.yunta/config.yaml (the user\n");
    out.push_str("# config layer), never in this file.\n\n");
    out.push_str("project:\n");
    out.push_str(&format!("  name: {project_name}\n"));
    out.push_str(&format!("  base_branch: {base_branch}\n"));
    out.push_str("  branch_prefix: yunta/\n\n");
    out.push_str(&render_detected(detected));

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
            yunta_api::machine::first_built_adapter()
        ));
    }
    out
}

/// What `init` found in the repository, as config: the commands a
/// workflow asks for by name, the suite a run measures when it reads the
/// measurement, and the forge `origin` is on.
fn render_detected(detected: &Detected) -> String {
    let mut out = String::new();
    if !detected.commands.is_empty() {
        out.push_str("# What this project runs for each capability a workflow names\n");
        out.push_str(&format!(
            "# (`run: {{ command: lint }}`), as `{}` detected it.\n",
            command::line(&NextStep::Init)
        ));
        out.push_str("commands:\n");
        for (name, text) in &detected.commands {
            out.push_str(&format!("  {name}: {}\n", yaml_string(text)));
        }
        out.push('\n');
    }
    if let Some(suite) = &detected.suite {
        out.push_str("# Measured before a run's first node when the run reads it.\n");
        out.push_str(&format!("baseline:\n  suite: {}\n\n", yaml_string(suite)));
    }
    if let Some(repo) = &detected.forge {
        out.push_str("# Where a `pull_request` node opens its pull request.\n");
        out.push_str(&format!(
            "forge:\n  github:\n    repo: {repo}\n    token_env: GITHUB_TOKEN\n\n"
        ));
    }
    out
}

/// `text` as a double-quoted YAML scalar.
fn yaml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The line `init` writes above the entries it adds to `.gitignore`, and
/// finds again so a second `init` adds nothing.
fn gitignore_marker() -> String {
    format!("# added by `{}`", command::line(&NextStep::Init))
}

/// Defensive only — `paths.runs`/`paths.worktrees` default to `~/.yunta`,
/// outside the repo entirely, so none of this exists in a repo with
/// default config. It's here for the day someone points `paths:` back
/// into the repo on purpose.
fn gitignore_block() -> String {
    format!(
        "\n{marker}: only matters if `paths:` ever points inside \
         this repo (defaults keep run state in ~/.yunta):\n.yunta/runs/\n\
         .yunta/worktrees/\n.yunta/*.db\n.yunta/*.db-*\n",
        marker = gitignore_marker()
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
    if existing.contains(&gitignore_marker()) {
        return Ok(false);
    }
    let mut updated = existing;
    updated.push_str(&gitignore_block());
    std::fs::write(&path, updated)?;
    Ok(true)
}

/// One setting asked for on `console`, or `default` when nobody
/// answers.
///
/// Answered on the one line every prompt in this binary is answered on
/// — same editing, same Escape, same Ctrl-C, same terminal handed back
/// — so a person who has answered a run answers `init` the same way.
/// An empty line takes the default, and so does Escape: "not me, not
/// now" about a setting that already has a detected value is that
/// value. `None` where Ctrl-C stopped `init`.
fn asked(console: &Console, prompt: &str, default: &str) -> Option<String> {
    let asking = Line::new()
        .push(Tone::Strong, prompt)
        .push(Tone::Muted, format!(" (enter keeps `{default}`)"));
    ask_line(console, &asking, default)
}

/// The project's name and base branch, as `init` answers them: asked on
/// `console` when there is one, each prompt keeping its detected value when
/// it is left empty, and the detected values as they stand otherwise. `None`
/// where a person stopped `init` at one of them: nothing more is asked.
fn project_answers(
    console: Option<&Console>,
    default_name: String,
    default_branch: String,
) -> Option<(String, String)> {
    match console {
        Some(console) => {
            // What Escape does, said once above the prompts it applies
            // to — the same place every other surface says it.
            // best-effort: the notice informs only; a refused write falls back to the defaults.
            let _ = console.say(&console.escape().said());
            Some((
                asked(console, "project name", &default_name)?,
                asked(console, "base branch", &default_branch)?,
            ))
        }
        None => Some((default_name, default_branch)),
    }
}

/// The console `init` asks on, when `-i` asked for one and a terminal
/// answers, and `None` otherwise. `-i` degrades with a warning rather than
/// hanging on a terminal that will never produce a line. There is no run
/// drawing here, so what opening the console has to say goes out through a
/// door onto nothing, which is stderr.
async fn open_console(interactive: bool) -> Option<Console> {
    let console = match interactive {
        true => Console::open(&Diagnostics::none(), Escape::KeepsDefault).await,
        false => None,
    };
    if interactive && console.is_none() {
        crate::ask::no_terminal("using detected defaults");
    }
    console
}

/// Refuses a project config that is already there, unless `force` says to
/// write it again.
fn refuse_overwrite(config_path: &Path, force: bool) -> Result<(), CliError> {
    if config_path.exists() && !force {
        return Err(CliError::msg(format!(
            "{} already exists; pass --force to overwrite",
            config_path.display()
        )));
    }
    Ok(())
}

pub async fn init(
    shell: &Shell,
    interactive: bool,
    force: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    let repo = shell.workspace().dir.clone();
    // `init` is the command that makes a project, so there is no
    // `Context` to resolve yet; its probes still spawn git, under the
    // shell's interruption, like any other invocation.
    let interrupt = shell.interrupt();
    let clock = shell.workspace().clock.as_ref();
    let env = shell.workspace().env.clone();
    let supervision = Supervision::outside_any_run(interrupt.stop(), clock).with_path(&env.path);

    let config_path = repo.join(".yunta/config.yaml");
    refuse_overwrite(&config_path, force)?;

    let console = open_console(interactive).await;

    let default_name = repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workflow-project".to_string());
    let default_branch = detect_base_branch(&repo, supervision).await?;
    let detected = yunta_api::machine::in_repo(shell.workspace(), &repo, &shell.stop()).await?;

    let Some((project_name, base_branch)) =
        project_answers(console.as_ref(), default_name, default_branch)
    else {
        return Ok(crate::output::Outcome::interrupted());
    };

    let probed = yunta_api::machine::probe_known_adapters().await;
    let config_yaml = render_config_yaml(&project_name, &base_branch, &probed, &detected);
    let (wrote, skipped) = write_project(&repo, &config_path, config_yaml, force).await?;

    let init = Initialized {
        repo: &repo,
        wrote: &wrote,
        skipped: &skipped,
        detected: &detected,
        probed: &probed,
    };
    crate::output::show(look, &init);
    Ok(Outcome::Success)
}

/// Writes what `init` puts in the project: the config first, then the
/// ignore entries and the mechanism skill. Answers what it wrote, and what
/// it left as it was.
async fn write_project(
    repo: &Path,
    config_path: &Path,
    config_yaml: String,
    force: bool,
) -> Result<(Vec<String>, Vec<String>), CliError> {
    let mut wrote = Vec::new();
    let mut skipped = Vec::new();

    if let Some(parent) = config_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| CliError::io("create", parent.display(), source))?;
    }
    // Never write a config this binary can't read back: parse the
    // generated text into a real `ConfigLayer` first, so a broken template
    // fails here instead of leaving an unreadable file — the more so under
    // `--force`, which would otherwise overwrite a good config with a bad
    // one.
    yunta_core::ConfigLayer::read(&config_yaml)
        .map_err(|e| CliError::msg(format!("the generated config is not valid: {e}")))?;
    tokio::fs::write(config_path, config_yaml)
        .await
        .map_err(|source| CliError::io("write", config_path.display(), source))?;
    wrote.push(".yunta/config.yaml".to_string());

    match write_gitignore(repo) {
        Ok(true) => wrote.push(".gitignore".to_string()),
        Ok(false) => skipped.push(".gitignore (already has yunta entries)".to_string()),
        Err(source) => return Err(CliError::io("update", ".gitignore", source)),
    }

    match skill::write_mechanism_skill(repo, force) {
        Ok(true) => wrote.push(format!("{MECHANISM_SKILL_DIR}/SKILL.md")),
        Ok(false) => skipped.push(format!(
            "{MECHANISM_SKILL_DIR}/SKILL.md (already exists, pass --force to rewrite)"
        )),
        Err(source) => return Err(CliError::io("write", "the mechanism skill", source)),
    }
    Ok((wrote, skipped))
}
