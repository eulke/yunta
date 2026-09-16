//! What a listener is given at birth, and what every listener of one run
//! has in common.
//!
//! The host is run-wide and outlives any single session: it carries the
//! run's identity, its own handle on the log, and the facts a tool needs
//! about the workflow that no session can be trusted to supply. The
//! access is the per-session cut of it — the node this listener speaks
//! for and the artifacts that node's close will verify — assembled by
//! the engine before the session spawns, which is what makes a tool call
//! about another run or another node unrepresentable rather than
//! rejected.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use yunta_core::{ArtifactSpec, Coordination, NodeId, NodeKind, RunId, Workflow};
use yunta_storage::AsyncStorage;

use crate::observer::RunObserver;

/// What every listener of one run shares: its own handle on the log
/// (the listener outlives any borrow of the engine's), the run
/// identity, and which nodes sit in a `coordination: blackboard`
/// group — for anyone else, the blackboard tools are never even
/// mounted.
/// What a host is built from: everything about the run that outlives
/// any one session. One value because it is read together, once, when
/// the run wakes.
pub struct HostOf {
    pub storage: AsyncStorage,
    pub run_id: RunId,
    pub clock: Arc<dyn yunta_core::Clock>,
    pub observer: Option<Arc<dyn RunObserver>>,
    pub run_dir: PathBuf,
    pub max_artifact_bytes: Option<u64>,
    pub redactor: yunta_core::Redactor,
}

pub struct RunToolsHost {
    pub(super) storage: AsyncStorage,
    pub(super) run_id: RunId,
    /// What the config named as a secret, taken out of every event this
    /// host's listeners append — the same door the run's own appends go
    /// through.
    pub(super) redactor: yunta_core::Redactor,
    /// The members of each `coordination: blackboard` group, by the
    /// group's own id and in declaration order — what the group
    /// consolidates when it closes, and what each of its members reads
    /// while it runs. One reading of the workflow, so no consumer can
    /// compute a second list.
    groups: HashMap<NodeId, Vec<NodeId>>,
    /// Which group a node belongs to — the mount rule for
    /// `yunta_get_blackboard`.
    member_of: HashMap<NodeId, NodeId>,
    /// Where the run keeps its artifacts. A session's working directory is
    /// the worktree, not this, so a tool that reads what the node declared
    /// has to be told.
    pub(super) run_dir: PathBuf,
    /// `limits.max_artifact_bytes`, so a check and the close answer the
    /// same about a runaway file.
    pub(super) max_artifact_bytes: Option<u64>,
    /// The run's injected clock — the listener stamps its own event
    /// appends with it, never a fresh `SystemClock`, so every emitter on
    /// the run shares one clock.
    pub(super) clock: Arc<dyn yunta_core::Clock>,
    /// The invocation's display surface, held by the same rule as the
    /// clock and the log handle beside it: a listener outlives every
    /// borrow of the engine's, so it owns its clone. What a session
    /// records mid-flight reaches a live view the moment it lands,
    /// because the log this host hands out carries the mirror.
    pub(super) observer: Option<Arc<dyn RunObserver>>,
}

impl RunToolsHost {
    pub fn new(workflow: &Workflow, host: HostOf) -> Self {
        let HostOf {
            storage,
            run_id,
            clock,
            observer,
            run_dir,
            max_artifact_bytes,
            redactor,
        } = host;
        let mut groups = HashMap::new();
        let mut member_of = HashMap::new();
        for node in &workflow.nodes {
            if let NodeKind::Parallel {
                coordination: Coordination::Blackboard,
                nodes: children,
                ..
            } = &node.kind
            {
                for child in children {
                    member_of.insert(child.id.clone(), node.id.clone());
                }
                groups.insert(
                    node.id.clone(),
                    children.iter().map(|c| c.id.clone()).collect(),
                );
            }
        }
        Self {
            storage,
            run_id,
            redactor,
            groups,
            member_of,
            clock,
            observer,
            run_dir,
            max_artifact_bytes,
        }
    }

    /// Whether `node` sits inside a `coordination: blackboard` group —
    /// the mount rule for `yunta_get_blackboard`, and the
    /// capability gate the engine checks before a session that would
    /// need it (a declared coordination the adapter can't carry is a
    /// node failure, never silent emulation).
    pub fn is_blackboard_member(&self, node: &NodeId) -> bool {
        self.member_of.contains_key(node)
    }

    /// The members of the `coordination: blackboard` group `node` is —
    /// in declaration order — or of the group it belongs to. Empty for
    /// a node that is neither.
    ///
    /// The group asks when it closes and a member asks while it runs,
    /// and both get the same list: what a group consolidates is exactly
    /// what its members could read.
    pub fn members_of(&self, node: &NodeId) -> &[NodeId] {
        let group = self.member_of.get(node).unwrap_or(node);
        self.groups.get(group).map_or(&[], Vec::as_slice)
    }
}

/// What a session needs to reach the run's own tools: the host, the node
/// the listener speaks for, and the artifacts that node's close will
/// verify.
///
/// The names are already rendered, so a check inside the session and the
/// verdict at close look at the same files. A node that declares none just
/// carries an empty list — the check tool then has nothing to offer and
/// says so.
#[derive(Clone)]
pub struct RunToolsAccess {
    pub host: Arc<RunToolsHost>,
    pub node: NodeId,
    /// What the node is, because who answers for an artifact it
    /// declared is the node's kind's to say — the same question its
    /// close asks, answered by the same function.
    pub node_kind: NodeKind,
    pub declared: Vec<ArtifactSpec>,
}
