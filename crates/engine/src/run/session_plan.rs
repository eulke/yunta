//! The one door a session opens through.
//!
//! A `SessionRequest` is the whole contract between the engine and an
//! adapter: what to run, on which model, with which permissions, which
//! skills, which tools, and where it may write. Written in two places it
//! became two contracts — a task session that reached its node's model
//! and agent only by accident, and a typed-artifact gate that guarded
//! prompt nodes and not loop nodes.
//!
//! So it is written here, once. [`SessionPlan`] carries what is this
//! session's alone; [`SessionSetup`] carries what its node resolved for
//! every session it opens; [`open_session`] is the only place in the
//! workspace that composes the two into a request.

use std::path::PathBuf;
use std::sync::Arc;

use yunta_core::fence::{Advice, Fence};
use yunta_core::port::{Adapter, Budget, PermissionProfile, SessionRequest};
use yunta_core::ScopeGlob;
use yunta_core::{Node, Task};

use crate::run::node_exec::NodeEnd;
use crate::run::runner_resolve::RunToolsSetupError;
use crate::run_tools::{RunToolsSession, TaskAccess};
use crate::task_cycle::{SessionObserver, SessionSetup};

/// Everything a node resolves once for every session it opens: which
/// skills mount, whether its sessions may hold the run's tools, the
/// secrets they see, and the runner they all run on.
///
/// Resolved here so the two node kinds that open sessions — a prompt
/// node and a loop node's tasks — resolve them the same way and degrade
/// the same way when the adapter cannot do one of them.
pub(crate) async fn resolve_setup(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
    chosen: &yunta_core::RunnerCandidate,
) -> Result<Result<SessionSetup, NodeEnd>, crate::run::RunError> {
    let adapter = &ctx.adapters[&chosen.adapter];
    let skills = match mount_skills(ctx, node, adapter.as_ref()).await? {
        Ok(skills) => skills,
        Err(end) => return Ok(Err(end)),
    };
    let run_tools = match tools_access(ctx, node, adapter.as_ref()).await? {
        Ok(access) => access,
        Err(end) => return Ok(Err(end)),
    };
    state_run_wide_absences(ctx, node, adapter.as_ref()).await?;
    Ok(Ok(SessionSetup {
        skills,
        adapter_settings: ctx.adapter_settings(&chosen.adapter),
        env: SessionSetup::secrets_env(&ctx.manifest.config, ctx.secrets.as_deref()),
        run_tools,
        fence_hook: ctx.fence_hook.clone(),
        run_dir: ctx.run_dir.to_path_buf(),
        node: node.id.clone(),
        chosen: chosen.clone(),
        artifact_dir: crate::run::node_exec::artifact_dir(ctx, node),
        run_tools_required: mandatory_tools(ctx, node),
    }))
}

/// The two capabilities whose absence is the run's condition rather
/// than a node's choice, stated once on the log the first time a node
/// asks for what they cover: a scope the adapter cannot hold the session
/// to as it edits, and a token cap it cannot count against.
///
/// A run that asks for neither says nothing about either.
async fn state_run_wide_absences(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
    adapter: &dyn Adapter,
) -> Result<(), crate::run::RunError> {
    if declares_edit_scope(node) {
        crate::run::capability::require(ctx, adapter, yunta_core::Capability::Fence, node).await?;
    }
    if ctx
        .manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_tokens_per_run)
        .is_some()
    {
        crate::run::capability::require(ctx, adapter, yunta_core::Capability::UsageReporting, node)
            .await?;
    }
    Ok(())
}

/// Whether this node's sessions carry edit constraints: its own declared
/// scope, or — for a loop — the scope every task it runs declares.
fn declares_edit_scope(node: &Node) -> bool {
    !node.scope.is_empty() || matches!(node.kind, yunta_core::NodeKind::Loop { .. })
}

/// The skill directories this node's sessions mount. Names are resolved
/// by the engine and a name that resolves to nothing fails the node;
/// mounting is the adapter's, and an adapter without the capability
/// degrades with an event rather than failing — a skill is added
/// instruction, never correctness.
async fn mount_skills(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
    adapter: &dyn Adapter,
) -> Result<Result<Vec<PathBuf>, NodeEnd>, crate::run::RunError> {
    let skills = match crate::skills::resolve_skills(
        &ctx.manifest.config,
        &ctx.manifest.workflow,
        node,
        ctx.worktree,
    ) {
        Ok(skills) => skills,
        Err(error) => {
            return Ok(Err(crate::run::node_close::fail(
                ctx,
                node,
                error.to_string(),
                false,
            )
            .await?))
        }
    };
    // A node that mounts none asked for nothing, so nothing is missing.
    if skills.is_empty() {
        return Ok(Ok(skills));
    }
    Ok(Ok(
        match crate::run::capability::require(ctx, adapter, yunta_core::Capability::Skills, node)
            .await?
        {
            crate::run::capability::Decision::Granted => skills,
            crate::run::capability::Decision::Degraded => Vec::new(),
            crate::run::capability::Decision::Refused(error) => return Err(error),
        },
    ))
}

/// Whether this node's sessions may hold the run's tools, and what they
/// would be allowed to check. A node whose declared document has no way
/// in — an interpreted artifact, or a `coordination: blackboard` group,
/// on an adapter that cannot be a client of the per-run endpoint — is
/// refused here, before any session opens. Each session then opens its
/// own listener from this access: a listener belongs to a session, and a
/// node that opens many owns none of them itself.
async fn tools_access(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
    adapter: &dyn Adapter,
) -> Result<Result<Option<crate::run_tools::RunToolsAccess>, NodeEnd>, crate::run::RunError> {
    // A node that cannot proceed without them never degrades: the
    // refusal names what has no other way in. Every other node takes the
    // tools when the adapter can hold them and runs without them when it
    // cannot — they are an offer, so their absence states nothing.
    if let Err(error) = crate::run::runner_resolve::run_tools_allowed(ctx, node, adapter) {
        return Ok(Err(crate::run::node_close::fail(
            ctx,
            node,
            error.to_string(),
            false,
        )
        .await?));
    }
    Ok(Ok(adapter
        .capabilities()
        .declares(yunta_core::Capability::RunTools)
        .then(|| crate::run_tools::RunToolsAccess {
            host: ctx.run_tools_host.clone(),
            node: node.id.clone(),
            node_kind: node.kind.clone(),
            declared: crate::run::node_exec::declared_artifacts(ctx, node),
        })))
}

/// What makes this node's tools mandatory rather than an offer.
fn mandatory_tools(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
) -> Option<crate::task_cycle::RunToolsNeed> {
    if ctx.run_tools_host.is_blackboard_member(&node.id) {
        return Some(crate::task_cycle::RunToolsNeed::Blackboard);
    }
    if let Some(kind) = crate::run::runner_resolve::declared_typed_artifact(ctx, node) {
        return Some(crate::task_cycle::RunToolsNeed::TypedArtifact(kind));
    }
    matches!(node.kind, yunta_core::NodeKind::Loop { .. })
        .then_some(crate::task_cycle::RunToolsNeed::Task)
}

/// What one session is: its prompt, where it works, and the decisions
/// that belong to it rather than to the node it serves.
pub(crate) struct SessionPlan<'a> {
    /// The node this session serves. A task session serves the loop
    /// node, not the task: the file it writes is the node's.
    pub node: &'a Node,
    /// The task this session was opened for, when a loop opened it:
    /// what its tools read and judge, the scope its edits are held to
    /// (declared plus granted), and the name of its own scratch slot.
    pub task: Option<Arc<TaskAccess>>,
    /// Already rendered, templates resolved. `open_session` appends the
    /// tool sentence when — and only when — this session holds tools.
    pub prompt: String,
    /// Where the session works: the run's worktree, or the task's own.
    pub cwd: PathBuf,
    /// What the session may do to the tree.
    pub profile: PermissionProfile,
    /// What it may spend.
    pub budget: Budget,
}

/// What stops a session from opening.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenSessionError {
    #[error(transparent)]
    Audit(#[from] yunta_storage::StorageError),
    #[error(transparent)]
    RunTools(#[from] RunToolsSetupError),
}

/// A session opened and waiting on its dispatch: the request to hand the
/// adapter, and the tools listener that must outlive the dispatch.
pub(crate) struct OpenedSession {
    pub request: SessionRequest,
    /// Held by the caller across the dispatch and dropped with it: the
    /// listener dies when the session does.
    pub run_tools: Option<RunToolsSession>,
}

/// Opens one session against `adapter`: mounts the per-session tools
/// listener when the node's setup allows one, tells the session what it
/// owes, and writes the request.
///
/// A listener that fails to bind degrades — the session runs without
/// tools and the log says so — unless the node cannot do without them:
/// a task session reads its task and checks its work nowhere else.
pub(crate) async fn open_session(
    setup: &SessionSetup,
    plan: SessionPlan<'_>,
    adapter: &dyn Adapter,
    observer: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
) -> Result<OpenedSession, OpenSessionError> {
    let run_tools = mount_tools(setup, &plan, adapter, observer).await?;
    let scratch_dir = scratch_dir(setup, &plan);
    let fence = fence(setup, &plan, run_tools.is_some());
    let task = plan.task.clone();

    let request = SessionRequest {
        prompt: told(setup, plan.prompt, run_tools.as_ref(), task.as_deref()),
        cwd: plan.cwd,
        model: Some(setup.chosen.model.clone()),
        agent: setup.chosen.agent.clone(),
        permissions: plan.profile,
        env: setup.env.clone(),
        fence,
        fence_hook: setup.fence_hook.clone(),
        budget: plan.budget,
        adapter_settings: setup.adapter_settings.clone(),
        skills: setup.skills.clone(),
        run_tools_endpoint: run_tools.as_ref().map(|session| session.endpoint.clone()),
        // The file this node closes on is written from what its
        // sessions hand over, so a session that has one to write is
        // told where it belongs.
        artifact_dir: setup.artifact_dir.clone(),
        scratch_dir,
    };
    // What the adapter stages for its own mechanics is known only now,
    // and no call can reach the session's tools before it is dispatched:
    // a check the session asks for leaves out exactly what its close will.
    if let Some(task) = &task {
        let _ = task.staged.set(adapter.staged_paths(&request));
    }
    Ok(OpenedSession { request, run_tools })
}

/// The session's prompt with every sentence its mount produces: which
/// documents to submit, and — for a task session — where its task is
/// read and how its work is judged. Produced by the mount, so a session
/// is never told to call something this adapter did not give it.
fn told(
    setup: &SessionSetup,
    mut prompt: String,
    run_tools: Option<&RunToolsSession>,
    task: Option<&TaskAccess>,
) -> String {
    let declared = setup
        .run_tools
        .as_ref()
        .map(|access| access.declared.as_slice())
        .unwrap_or_default();
    let notices = [
        crate::run_tools::submission_notice(run_tools, declared, setup.artifact_dir.as_deref()),
        crate::run_tools::task_notice(run_tools, task),
    ];
    for notice in notices.into_iter().flatten() {
        prompt.push_str(&notice);
    }
    prompt
}

/// What this session may write: its profile, the scope it works to — a
/// task session its task's, the expansions already granted included; a
/// node's own session the node's — and the one directory outside the
/// worktree its declared files belong in.
///
/// A session that mounted the scope-expansion tool is told to ask for
/// more when it is refused; one that did not is told to report the need
/// and move on, because asking is not something it can do.
fn fence(setup: &SessionSetup, plan: &SessionPlan<'_>, holds_run_tools: bool) -> Fence {
    // Scope expansion is task-keyed, so only a task session is offered
    // the tool that asks for it: every other session is told to report
    // the need instead of asking with something it does not hold.
    let advice = if holds_run_tools && plan.task.is_some() {
        Advice::RequestExpansion
    } else {
        Advice::ReportFinding
    };
    let scope: Option<&[ScopeGlob]> = match &plan.task {
        Some(task) => Some(&task.scope),
        None => (!plan.node.scope.is_empty()).then_some(plan.node.scope.as_slice()),
    };
    Fence::for_session(
        plan.profile,
        scope,
        &[],
        setup.artifact_dir.as_deref(),
        advice,
    )
}

/// Where this session may drop its own scaffolding. Concurrent sessions
/// of one node never scaffold over each other, so a task's slot carries
/// its own id.
fn scratch_dir(setup: &SessionSetup, plan: &SessionPlan<'_>) -> PathBuf {
    match &plan.task {
        Some(task) => crate::session_dir::SessionSlot::Task(&setup.node, &task.task.id)
            .scratch_dir(&setup.run_dir),
        None => crate::session_dir::SessionSlot::Node(&setup.node).scratch_dir(&setup.run_dir),
    }
}

/// This session's own listener on the run's tools, when its node's setup
/// allows one.
///
/// A node that cannot proceed without the tools fails here, before a
/// token is spent; for any other node a bind failure is recorded and the
/// session runs on — the tools are an offer, the node's own criteria are
/// the contract.
async fn mount_tools(
    setup: &SessionSetup,
    plan: &SessionPlan<'_>,
    adapter: &dyn Adapter,
    observer: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
) -> Result<Option<RunToolsSession>, OpenSessionError> {
    let Some(access) = &setup.run_tools else {
        return Ok(None);
    };
    let source = match crate::run_tools::open_session_listener(
        access.clone(),
        plan.task.clone(),
        plan.cwd.clone(),
    )
    .await
    {
        Ok(session) => return Ok(Some(session)),
        Err(source) => source,
    };
    if let Some(need) = &setup.run_tools_required {
        return Err(OpenSessionError::RunTools(match need {
            crate::task_cycle::RunToolsNeed::Blackboard => RunToolsSetupError::ListenerFailed {
                node: plan.node.id.clone(),
                source,
            },
            crate::task_cycle::RunToolsNeed::TypedArtifact(kind) => {
                RunToolsSetupError::TypedArtifactListenerFailed {
                    node: plan.node.id.clone(),
                    kind: *kind,
                    source,
                }
            }
            crate::task_cycle::RunToolsNeed::Task => RunToolsSetupError::TaskListenerFailed {
                node: plan.node.id.clone(),
                source,
            },
        }));
    }
    if let Some((observer, node)) = observer {
        observer
            .record(
                node,
                yunta_core::events::EventPayload::Session(
                    yunta_core::events::SessionEvent::CapabilityDegraded(
                        yunta_core::events::CapabilityDegradedPayload::new(
                            yunta_core::Capability::RunTools,
                            adapter.id().clone(),
                            yunta_core::events::Policy::NoRunTools,
                        ),
                    ),
                ),
            )
            .await?;
    }
    Ok(None)
}

/// The brief a task session gets: the node's instruction and which task
/// is this session's — never the task's contents. Its scope, criteria and
/// notes live in the run's tasks document, and the session reads them
/// there through its tools, so what it is told and what it is judged by
/// can never be two copies that disagree.
pub(crate) fn task_brief(instruction: &str, task: &Task) -> String {
    format!(
        "{instruction}\n\nYour task: `{}` — {}.",
        task.id, task.title
    )
}
