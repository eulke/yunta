//! `yunta run <workflow>`: resolve config, check, freeze the manifest,
//! prepare the run's isolated working tree and create the run.
//!
//! A run takes one of two shapes from there, one module each —
//! [`attached`] drives it here, [`detach`] hands it to a child that
//! outlives this process — and every step both of them share lives in
//! this module, so the two reach a run through the very same resolution,
//! refusals and creation.
//!
//! The run id is a ULID from the shell's id source; the engine mints
//! only the ids of the runs this one gives birth to, through the same
//! injected source.

use std::collections::HashMap;
use std::path::Path;

use yunta_api::machine::scripts_sessions;
use yunta_api::{Admission, CreatedRun, Project, Remark, RunRequest, Stop};
use yunta_core::step::NextStep;
use yunta_core::{AdapterId, InputName, ModeName, RunAdapter};
use yunta_render::command;

mod attached;
mod detach;

use crate::error::CliError;
use crate::output::Mode;
use crate::output::Outcome;
use crate::shell::Shell;
use surco::look::Look;

/// Parses `--input name=value` entries into the raw map
/// `yunta_engine::resolve_inputs` validates against the workflow's own
/// `inputs:` — this function only enforces the *syntax* of the flag
/// (exactly one `=`, non-empty name); everything about whether
/// a name is declared, required, or well-typed is `resolve_inputs`'s
/// job, not this one's, so the two error paths never disagree about who
/// owns which rule.
fn parse_inputs(raw: &[String]) -> Result<HashMap<InputName, String>, String> {
    let mut inputs = HashMap::new();
    for entry in raw {
        let (name, value) = entry
            .split_once('=')
            .ok_or_else(|| format!("--input `{entry}` must have the form `name=value`"))?;
        let name: InputName = name
            .parse()
            .map_err(|error| format!("--input `{entry}`: {error}"))?;
        if inputs.insert(name.clone(), value.to_string()).is_some() {
            return Err(format!("--input `{name}` was given more than once"));
        }
    }
    Ok(inputs)
}

/// Creates a run from a workflow and takes one of the two shapes a run
/// reaches a person in: driven here, with this invocation watching it to
/// its end, or handed to a detached child that outlives this process.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    shell: &Shell,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    fixture: Option<&Path>,
    mode: Option<&ModeName>,
    output: Mode,
    detach: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let stop = shell.stop();
    let mock_fixture = mock_fixture(adapter, fixture, detach)?;

    if detach {
        return detach::detached(detach::Detaching {
            project: &project,
            stop: &stop,
            workflow_path,
            raw_inputs,
            adapter,
            mode,
            output,
            look,
        })
        .await;
    }

    attached::attached(attached::Attaching {
        project: &project,
        stop: &stop,
        workflow_path,
        raw_inputs,
        adapter,
        mock_fixture,
        mode,
        output,
        look,
    })
    .await
}

/// `--adapter mock --fixture <path>`: the scripted fixture every session
/// runs against, with no real adapter constructed or probed.
///
/// The one place the flags around a mock run mean something: each
/// combination is either that fixture or a refusal naming what to do
/// instead, read before the workflow is even loaded so a misuse costs
/// nothing.
fn mock_fixture<'a>(
    adapter: Option<&AdapterId>,
    fixture: Option<&'a Path>,
    detach: bool,
) -> Result<Option<&'a Path>, CliError> {
    match (adapter, fixture) {
        // A fixture is read by whoever runs the sessions, and `--detach`
        // hands the run to another process, which resolves the adapters
        // `runners:` names and takes no fixture of its own.
        (Some(id), _) if scripts_sessions(id) && detach => Err(CliError::msg(format!(
            "`--adapter mock` scripts every session from a fixture, and `--detach` hands \
             the run to another process that resolves the adapters `runners:` names and \
             reads no fixture; drop `--detach` to run against a fixture here, or write a \
             case and run `{test}`",
            test = command::line(&NextStep::Test { dir: None })
        ))),
        (Some(id), Some(path)) if scripts_sessions(id) => Ok(Some(path)),
        (Some(id), None) if scripts_sessions(id) => Err(CliError::msg(format!(
            "`--adapter mock` runs the workflow against a scripted fixture; pass \
             `--fixture <path>` (the format a `.yunta/tests/` fixture uses), or write a \
             case and run `{test}`",
            test = command::line(&NextStep::Test { dir: None })
        ))),
        (_, Some(_)) => Err(CliError::msg(
            "`--fixture` only applies together with `--adapter mock`",
        )),
        _ => Ok(None),
    }
}

/// The run `workflow_path` asks for here, admitted by the API: its inputs read
/// off their flags, its sessions pinned to `adapter` or scripted by
/// `mock_fixture`, each caution and the estimate said the way `output` asks
/// as the API says them.
pub(crate) async fn admitted<'p>(
    project: &'p Project,
    stop: &Stop,
    asked: Asked<'_>,
    output: Mode,
    look: &Look,
) -> Result<Admission<'p>, CliError> {
    let inputs = parse_inputs(asked.raw_inputs).map_err(CliError::msg)?;
    let adapter = match asked.mock_fixture {
        Some(path) => Some(RunAdapter::Mock {
            fixture: yunta_api::machine::read_fixture(path).await?,
        }),
        None => asked.adapter.map(|id| RunAdapter::Named(id.clone())),
    };
    let request = RunRequest {
        workflow: asked.workflow_path.to_path_buf(),
        inputs,
        mode: asked.mode.cloned(),
        adapter,
        hermetic: asked.hermetic,
    };
    let say = |remark: Remark| crate::remarks::say(remark, output, look);
    Ok(project.admit(request, &say, stop).await?)
}

/// What a run is asked for with on the command line: the workflow reference,
/// the `--input` flags, the adapter or the fixture its sessions run on, and
/// the mode it starts in.
pub(crate) struct Asked<'a> {
    pub(crate) workflow_path: &'a Path,
    pub(crate) raw_inputs: &'a [String],
    pub(crate) adapter: Option<&'a AdapterId>,
    pub(crate) mock_fixture: Option<&'a Path>,
    pub(crate) mode: Option<&'a ModeName>,
    /// Whether the run freezes none of the person's own knowledge.
    pub(crate) hermetic: bool,
}

/// The admitted run, created by the API, what it says while it does said the
/// way `output` asks: the shared create step of an executed run and a detached
/// one. Prints nothing of its own: the caller reports what it made.
pub(crate) async fn create_run_from<'p>(
    admission: Admission<'p>,
    stop: &Stop,
    output: Mode,
    look: &Look,
) -> Result<CreatedRun<'p>, CliError> {
    let say = |remark: Remark| crate::remarks::say(remark, output, look);
    Ok(admission.create(&say, stop).await?)
}
