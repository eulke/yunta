//! What a `kind: workflow` node checks ahead of its child: the depth it
//! may nest to, the workflow text it reads for the child, the static check of
//! that workflow as it is mounted, the inputs it hands over and the artifacts
//! it mounts. A refusal ends the node, with nothing born or linked behind it.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use yunta_core::events::{ChildRefusal, Failure, TokenUsage};
use yunta_core::template::render_template;
use yunta_core::{InputName, MountSpec, Node, Workflow};

use crate::frozen::read::{ChildWorkflow, Source};
use crate::frozen::FrozenError;
use crate::replay::RunView;
use crate::run::BirthArtifact;

use super::super::node_close::{fail, fail_with};
use super::super::node_exec::template_vars;
use super::super::session_policy::close_frozen;
use super::super::{RunCtx, RunError};
use super::mounts::resolve_mounts;
use super::{child_refused, ended, Step, WorkflowCall};

/// The failure a `use:` nested past the configured depth fails its node with,
/// or `None` when the child may be born at this depth.
pub(super) fn nested_too_deep(ctx: &RunCtx<'_>, use_name: &str) -> Option<Failure> {
    // The configurable max depth, enforced where the depth
    // actually grows — `check`'s static walk covers the files as they
    // are at check time; this guard covers what the run really loads.
    let max_depth = ctx.manifest.config.resolved_max_workflow_depth();
    (ctx.depth + 1 > max_depth).then(|| Failure::NestedTooDeep {
        workflow: use_name.to_string(),
        depth: ctx.depth + 1,
        max_depth,
    })
}

/// What a child is admitted with: the text read for it and the workflow that
/// text parses to, the inputs it is handed, and the artifacts its node mounts
/// into it.
pub(super) struct Admitted<'a> {
    pub(super) child: ChildWorkflow<'a>,
    pub(super) workflow: Workflow,
    pub(super) provided: HashMap<InputName, String>,
    pub(super) mounted: Vec<BirthArtifact>,
}

/// Admits the child `call` names, as its node would birth it: the text read
/// for it, the static check of its workflow as it is mounted, its inputs and
/// its mounts. A refusal ends the node.
pub(super) async fn admit<'a>(
    ctx: &RunCtx<'_>,
    node: &Node,
    call: &WorkflowCall<'_>,
    source: &'a Source<'_>,
    view: &RunView,
) -> Result<Step<Admitted<'a>>, RunError> {
    let (child, workflow) = match read_child(ctx, node, source, call.use_name).await? {
        Step::Proceed(read) => read,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    // The same static gate `yunta run` applies before spending anything
    // — a child born broken is refused at birth, with the check's own
    // diagnostics — asked of the child as it is born: holding what this
    // node mounts into it.
    if let Some(rules) = check_child(ctx, &workflow, call.mounts) {
        let refusal = ChildRefusal::FailsCheck { rules };
        return ended(fail(ctx, node, child_refused(call.use_name, refusal)).await);
    }
    let provided = match render_inputs(ctx, node, call.inputs).await? {
        Step::Proceed(provided) => provided,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    // Mounts resolve to bytes here, before anything is linked or
    // born — a missing source is this node's failure, with no dangling
    // child left behind.
    let mounted = match resolve_mounts(ctx, &view.state, &view.events, call.mounts).await {
        Ok(mounted) => mounted,
        Err(problem) => {
            let end = fail_with(ctx, node, problem.into(), TokenUsage::default()).await;
            return ended(end);
        }
    };
    Ok(Step::Proceed(Admitted {
        child,
        workflow,
        provided,
        mounted,
    }))
}

/// The child `use_name` names, read from `source` and parsed. A refusal ends
/// the node.
async fn read_child<'a>(
    ctx: &RunCtx<'_>,
    node: &Node,
    source: &'a Source<'_>,
    use_name: &str,
) -> Result<Step<(ChildWorkflow<'a>, Workflow)>, RunError> {
    // Fresh birth: the child's text is the one this run reads for its
    // child — the copy its parent froze at birth, or the checkout's file
    // for a run that predates frozen inputs.
    let child = match source.workflow(use_name).await {
        Ok(child) => child,
        Err(FrozenError::Unresolved(problem)) => {
            let refusal = ChildRefusal::Unresolved { problem };
            return ended(fail(ctx, node, child_refused(use_name, refusal)).await);
        }
        Err(e) => return ended(close_frozen(ctx, node, e).await),
    };
    let workflow = match yunta_core::workflow::read::read(child.text(), Path::new(use_name)) {
        Ok(workflow) => workflow,
        Err(report) => {
            let refusal = ChildRefusal::Invalid { report };
            return ended(fail(ctx, node, child_refused(use_name, refusal)).await);
        }
    };
    Ok(Step::Proceed((child, workflow)))
}

/// The inputs the node hands its child, each rendered in the node's own
/// template scope. A template that does not render ends the node.
async fn render_inputs(
    ctx: &RunCtx<'_>,
    node: &Node,
    inputs: &BTreeMap<InputName, String>,
) -> Result<Step<HashMap<InputName, String>>, RunError> {
    // The parent's frozen contribution: the declared inputs, rendered
    // in the parent's own template scope.
    let vars = template_vars(ctx, node);
    let mut provided: HashMap<InputName, String> = Default::default();
    for (name, template) in inputs {
        match render_template(template, &vars) {
            Ok(value) => {
                provided.insert(name.clone(), value);
            }
            Err(error) => {
                let failure = Failure::Template {
                    template: error,
                    input: Some(name.clone()),
                };
                return ended(fail(ctx, node, failure).await);
            }
        }
    }
    Ok(Step::Proceed(provided))
}

/// The child's static check, asked of it as it is born with what its node
/// mounts into it: the code of each diagnostic that fails it, or `None` when
/// it passes. Each adapter the check names is read as the run holds it, with
/// the sandbox its edits run under.
fn check_child(
    ctx: &RunCtx<'_>,
    child: &yunta_core::Workflow,
    mounts: &[MountSpec],
) -> Option<Vec<String>> {
    let errors = crate::check::check_mounted(
        child,
        &ctx.manifest.config,
        &|adapter| {
            ctx.adapters
                .get(adapter)
                .map(|adapter| crate::check::AdapterFacts {
                    capabilities: adapter.capabilities(),
                    edit_sandbox: adapter.edit_sandbox(),
                })
        },
        mounts,
    );
    if errors.is_empty() {
        return None;
    }
    Some(errors.iter().map(|e| e.to_string()).collect())
}
