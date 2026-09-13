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

/// What every listener of one run shares: its own handle on the log
/// (the listener outlives any borrow of the engine's), the run
/// identity, and which nodes sit in a `coordination: blackboard`
/// group — for anyone else, the blackboard tools are never even
/// mounted.
pub struct RunToolsHost {
    pub(super) storage: AsyncStorage,
    pub(super) run_id: RunId,
    pub(super) blackboard_members: HashMap<NodeId, Vec<NodeId>>,
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
}

impl RunToolsHost {
    pub fn new(
        storage: AsyncStorage,
        run_id: RunId,
        workflow: &Workflow,
        clock: Arc<dyn yunta_core::Clock>,
        run_dir: PathBuf,
        max_artifact_bytes: Option<u64>,
    ) -> Self {
        let mut blackboard_members = HashMap::new();
        for node in &workflow.nodes {
            if let NodeKind::Parallel {
                coordination: Coordination::Blackboard,
                nodes: children,
                ..
            } = &node.kind
            {
                let member_ids: Vec<NodeId> = children.iter().map(|c| c.id.clone()).collect();
                for child in children {
                    blackboard_members.insert(child.id.clone(), member_ids.clone());
                }
            }
        }
        Self {
            storage,
            run_id,
            blackboard_members,
            clock,
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
        self.blackboard_members.contains_key(node)
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
    pub declared: Vec<ArtifactSpec>,
}
