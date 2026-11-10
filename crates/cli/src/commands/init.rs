//! `yunta init`: prepares a repo for Yunta once, through the API, which
//! detects its ecosystem, test command, base branch, the adapter CLIs this
//! machine answers for and the runners its workflows propose, and writes
//! `.yunta/config.yaml`, defensive `.gitignore` entries and the mechanism
//! skill. Non-interactive by default: every step picks the best answer it can
//! detect and reports it, and the runners stay proposed, commented;
//! `-i/--interactive` asks the name and base branch, and each runner's adapter
//! and model, which writes it declared. It degrades to non-interactive with a
//! warning when stdin isn't a TTY: `init` must never hang waiting for input
//! that isn't coming.

use std::collections::BTreeMap;

use surco::look::Look;
use yunta_api::{DetectedProject, ProjectSetup, RunnerDeclaration};
use yunta_core::{AdapterId, ModelName, RunnerCandidate, RunnerName};
use yunta_render::page::Initialized;
use yunta_render::prompt::{
    keeping, model_of, runs_on, Escape, BASE_BRANCH, PROJECT_NAME, USING_DETECTED_DEFAULTS,
};
use yunta_ui::prompt::Alternative;

use crate::ask::{ask_line, choose, Console, NoAnswer};
use crate::error::CliError;
use crate::output::Outcome;
use crate::shell::Shell;
use crate::surface::Diagnostics;

pub async fn init(
    shell: &Shell,
    interactive: bool,
    force: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    let workspace = shell.workspace();
    let detected = workspace.detect_project(force, &shell.stop()).await?;
    let console = open_console(interactive).await;
    let defaults = (detected.name.clone(), detected.base_branch.clone());
    let Some((name, base_branch)) = project_answers(console.as_ref(), defaults) else {
        return Ok(Outcome::interrupted());
    };
    let runners = match &console {
        Some(console) => match runner_answers(console, &detected) {
            Some(runners) => runners,
            None => return Ok(Outcome::interrupted()),
        },
        None => RunnerDeclaration::Proposed,
    };
    let setup = ProjectSetup {
        name,
        base_branch,
        runners,
    };
    let initialized = detected.write(setup).await?;
    let page = Initialized {
        repo: &workspace.dir,
        wrote: &initialized.wrote,
        skipped: &initialized.skipped,
        detected: &initialized.detected,
        probed: &initialized.probed,
        declared: &initialized.declared,
        undeclared: &initialized.undeclared,
    };
    crate::output::show(look, &page);
    Ok(Outcome::Success)
}

/// One setting asked for on `console`, or `default` when nobody
/// answers.
///
/// Answered on the one line every prompt in this binary is answered on
/// (same editing, same Escape, same Ctrl-C, same terminal handed back), so
/// a person who has answered a run answers `init` the same way. An empty
/// line takes the default, and so does Escape: "not me, not now" about a
/// setting that already has a detected value is that value. `None` where
/// Ctrl-C stopped `init`.
fn asked(console: &Console, prompt: &str, default: &str) -> Option<String> {
    ask_line(console, &keeping(prompt, default), default)
}

/// The project's name and base branch, as `init` answers them: asked on
/// `console` when there is one, each prompt keeping its detected value when
/// it is left empty, and the detected `defaults` as they stand otherwise.
/// `None` where a person stopped `init` at one of them: nothing more is
/// asked.
fn project_answers(
    console: Option<&Console>,
    (default_name, default_branch): (String, String),
) -> Option<(String, String)> {
    match console {
        Some(console) => {
            // What Escape does, said once above the prompts it applies
            // to: the same place every other surface says it.
            // best-effort: the notice informs only; a refused write falls back to the defaults.
            let _ = console.say(&console.escape().said());
            Some((
                asked(console, PROJECT_NAME, &default_name)?,
                asked(console, BASE_BRANCH, &default_branch)?,
            ))
        }
        None => Some((default_name, default_branch)),
    }
}

/// The runners a person declares on `console`: each one the repository proposes,
/// on an adapter this machine answers for, the first standing, with the model
/// typed. A runner whose model is left empty stays proposed, and with no adapter
/// answering nothing is asked. `None` where a person stopped `init`.
fn runner_answers(console: &Console, detected: &DetectedProject) -> Option<RunnerDeclaration> {
    let answered: Vec<&AdapterId> = detected
        .probed
        .iter()
        .filter(|adapter| adapter.healthy)
        .map(|adapter| &adapter.id)
        .collect();
    let Some(first) = answered.first() else {
        return Some(RunnerDeclaration::Proposed);
    };
    let mut declared = BTreeMap::new();
    for runner in &detected.proposed.roles {
        let adapter = adapter_of(console, runner, &answered)?.unwrap_or_else(|| (*first).clone());
        if let Some(model) = model_of_runner(console, runner)? {
            let candidate = RunnerCandidate {
                adapter,
                model,
                agent: None,
                effort: None,
            };
            declared.insert(runner.clone(), candidate);
        }
    }
    Some(match declared.is_empty() {
        true => RunnerDeclaration::Proposed,
        false => RunnerDeclaration::Declared(declared),
    })
}

/// The adapter `runner` runs on, picked among those that `answered`: `Some(None)`
/// where nothing was picked and the first stands, `None` where a person stopped
/// `init`.
fn adapter_of(
    console: &Console,
    runner: &RunnerName,
    answered: &[&AdapterId],
) -> Option<Option<AdapterId>> {
    let alternatives = answered
        .iter()
        .map(|adapter| Alternative::named(adapter.as_str(), (*adapter).clone()))
        .collect();
    // best-effort: the question informs only; the list is asked either way.
    let _ = console.say(&runs_on(runner));
    match choose(console, alternatives) {
        Ok(adapter) => Some(Some(adapter)),
        Err(NoAnswer::Interrupted) => None,
        Err(_) => Some(None),
    }
}

/// The model `runner` runs, as typed: `Some(None)` where the line was left empty,
/// and asked again where what was typed is no model name; `None` where a person
/// stopped `init`.
fn model_of_runner(console: &Console, runner: &RunnerName) -> Option<Option<ModelName>> {
    loop {
        let typed = ask_line(console, &model_of(runner), "")?;
        if typed.is_empty() {
            return Some(None);
        }
        match typed.parse::<ModelName>() {
            Ok(model) => return Some(Some(model)),
            Err(refused) => {
                // best-effort: the refusal informs only; the model is asked again.
                let _ = console.say(&refused.to_string());
            }
        }
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
        crate::ask::no_terminal(USING_DETECTED_DEFAULTS);
    }
    console
}
