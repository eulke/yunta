//! `yunta init`: prepares a repo for Yunta once, through the API, which
//! detects its ecosystem, test command, base branch and the adapter CLIs this
//! machine answers for, and writes `.yunta/config.yaml`, defensive
//! `.gitignore` entries and the mechanism skill. Non-interactive by default:
//! every step picks the best answer it can detect and reports it;
//! `-i/--interactive` only adds confirmation prompts here, and degrades to
//! non-interactive with a warning when stdin isn't a TTY: `init` must never
//! hang waiting for input that isn't coming.

use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_render::page::Initialized;

use crate::ask::{ask_line, Console, Escape};
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
    let initialized = detected.write(&name, &base_branch).await?;
    let page = Initialized {
        repo: &workspace.dir,
        wrote: &initialized.wrote,
        skipped: &initialized.skipped,
        detected: &initialized.detected,
        probed: &initialized.probed,
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
    let asking = Line::new()
        .push(Tone::Strong, prompt)
        .push(Tone::Muted, format!(" (enter keeps `{default}`)"));
    ask_line(console, &asking, default)
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
