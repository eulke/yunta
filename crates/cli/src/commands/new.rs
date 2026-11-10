//! `yunta new <name> [--shape ...]`: writes a new workflow from its skeleton,
//! through the API, and says the verdict `yunta check` reaches on it, exit
//! code included: a skeleton the project's config cannot run is written, and
//! said not to run. The shape is the one `--shape` names, or the one a person
//! picks under `--interactive`, or `one-node`.

use surco::look::Look;
use yunta_api::WorkflowStem;
use yunta_core::WorkflowShape;
use yunta_render::page::NewWorkflow;
use yunta_render::prompt::{defaulting_to, shape_does, Escape, CHOOSE_A_SHAPE};
use yunta_render::refusal::Refused;
use yunta_ui::prompt::Alternative;

use crate::ask::{choose, Console, NoAnswer};
use crate::error::CliError;
use crate::output::show;
use crate::output::Outcome;
use crate::shell::Shell;
use crate::surface::Diagnostics;

pub async fn new_workflow(
    shell: &Shell,
    name: &str,
    shape: Option<&str>,
    interactive: bool,
    force: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    let stem = WorkflowStem::parse(name)?;
    let shape = choose_shape(shape, interactive).await?;
    let workspace = shell.workspace();
    let written = workspace.new_workflow(&stem, shape, force).await?;
    let at = surco::paths::shown(&written.path, &workspace.dir, None);
    let page = |diagnosis| NewWorkflow {
        name,
        shape: shape.label(),
        path: &written.path,
        at: &at,
        diagnosis,
    };
    // The file is on disk whatever stopped the check of it.
    let diagnosis = written
        .diagnosis(&shell.stop())
        .await
        .inspect_err(|_| show(look, &page(None)))?;
    show(look, &page(Some(&diagnosis)));
    Ok(match diagnosis.errors() {
        0 => Outcome::Success,
        _ => Outcome::Reported,
    })
}

/// The shape picked off the one list this binary puts choices on (same
/// arrows, same filter, same Escape), or the first shape when nobody picks:
/// a skeleton is a starting point, and "not me, not now" about which one is
/// the plainest one. What the list is for is said above it first, and a
/// console that cannot be drawn on takes the first shape the same way.
fn picked(console: &Console) -> WorkflowShape {
    let choices = WorkflowShape::all()
        .into_iter()
        .map(|shape| Alternative {
            label: Some(shape_does(shape).to_string()),
            ..Alternative::named(shape.label(), shape)
        })
        .collect();
    console
        .say(CHOOSE_A_SHAPE)
        .map_err(NoAnswer::from)
        .and_then(|()| choose(console, choices))
        .unwrap_or(WorkflowShape::OneNode)
}

/// The shape a new workflow starts as: the one `--shape` names, or else the
/// one a person picks on the console under `--interactive`, or `one-node`.
async fn choose_shape(shape: Option<&str>, interactive: bool) -> Result<WorkflowShape, CliError> {
    match shape {
        Some(raw) => Ok(WorkflowShape::parse(raw).map_err(Refused::Setup)?),
        // There is no run drawing here, so what opening the console has
        // to say goes out through a door onto nothing, which is stderr.
        None => Ok(match interactive {
            false => WorkflowShape::OneNode,
            true => match Console::open(&Diagnostics::none(), Escape::TakesTheFirst).await {
                Some(console) => picked(&console),
                None => {
                    crate::ask::no_terminal(&defaulting_to(WorkflowShape::OneNode.label()));
                    WorkflowShape::OneNode
                }
            },
        }),
    }
}
