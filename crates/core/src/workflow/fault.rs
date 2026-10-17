//! What a workflow's rules found wrong with it, as the values each clause
//! names. The clause is what a reader acts on; the code is the stable name a
//! receipt counts the rule by.

use serde::{Deserialize, Serialize};

use crate::diagnostic::RuleCode;
use crate::{ModeName, NodeId, ScopeGlob};

/// What a rule of a workflow found wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowFault {
    InvariantInParallel {
        node: NodeId,
        group: NodeId,
    },
    DuplicateNode,
    /// `nearest` is the id the typed one most likely misspells, among the
    /// workflow's ids other than the node's own.
    UnknownReference {
        field: String,
        target: NodeId,
        nearest: Option<NodeId>,
    },
    /// `node` and `other` are the two children whose scopes overlap; the
    /// diagnostic's subject is their group.
    OverlappingParallelScope {
        node: NodeId,
        glob: ScopeGlob,
        other: NodeId,
        other_glob: ScopeGlob,
    },
    UnknownModeNode {
        mode: ModeName,
        node: NodeId,
    },
    InvariantLeftOut {
        mode: ModeName,
    },
    RerouteLeftOut {
        field: String,
        target: NodeId,
        mode: ModeName,
    },
    SourceLeftOut {
        field: String,
        source: NodeId,
        mode: ModeName,
    },
    OptionalInParallel {
        node: NodeId,
        group: NodeId,
    },
    ReroutesToOptional {
        target: NodeId,
    },
    ReadsWhatMayGo {
        field: String,
        source: NodeId,
        node: NodeId,
    },
}

impl WorkflowFault {
    /// The stable name of the rule this fault breaks.
    pub fn code(&self) -> RuleCode {
        match self {
            Self::InvariantInParallel { .. } => RuleCode::InvariantInParallel,
            Self::DuplicateNode => RuleCode::DuplicateId,
            Self::UnknownReference { .. } | Self::UnknownModeNode { .. } => {
                RuleCode::UnknownDependency
            }
            Self::OverlappingParallelScope { .. } => RuleCode::OverlappingScope,
            Self::InvariantLeftOut { .. }
            | Self::RerouteLeftOut { .. }
            | Self::SourceLeftOut { .. } => RuleCode::IncoherentMode,
            Self::OptionalInParallel { .. }
            | Self::ReroutesToOptional { .. }
            | Self::ReadsWhatMayGo { .. } => RuleCode::IncoherentOptional,
        }
    }

    /// The clause a reader acts on, in the words the rule has always been
    /// written in.
    pub fn clause(&self) -> String {
        match self {
            Self::InvariantInParallel { node, group } => invariant_in_parallel(node, group),
            Self::DuplicateNode => DUPLICATE_NODE.to_string(),
            Self::UnknownReference {
                field,
                target,
                nearest,
            } => unknown_reference(field, target, nearest.as_ref()),
            Self::OverlappingParallelScope {
                node,
                glob,
                other,
                other_glob,
            } => overlapping_parallel(node, glob, other, other_glob),
            Self::UnknownModeNode { mode, node } => unknown_mode_node(mode, node),
            Self::InvariantLeftOut { mode } => invariant_left_out(mode),
            Self::RerouteLeftOut {
                field,
                target,
                mode,
            } => reroute_left_out(field, target, mode),
            Self::SourceLeftOut {
                field,
                source,
                mode,
            } => source_left_out(field, source, mode),
            Self::OptionalInParallel { node, group } => optional_in_parallel(node, group),
            Self::ReroutesToOptional { target } => reroutes_to_optional(target),
            Self::ReadsWhatMayGo {
                field,
                source,
                node,
            } => reads_what_may_go(field, source, node),
        }
    }
}

const DUPLICATE_NODE: &str =
    "another node already carries this id; every id is declared once, `parallel` children included";

fn invariant_in_parallel(node: &NodeId, group: &NodeId) -> String {
    format!(
        "`{node}` is a child of the `parallel` group `{group}` and declares `invariant: \
         true`, which only a top-level node can honor; move it out of the group"
    )
}

fn unknown_reference(field: &str, target: &NodeId, nearest: Option<&NodeId>) -> String {
    format!(
        "`{field}` names `{target}`, and no node carries that id{}",
        crate::text::did_you_mean(target.as_str(), nearest.map(NodeId::as_str))
    )
}

fn overlapping_parallel(
    node: &NodeId,
    glob: &ScopeGlob,
    other: &NodeId,
    other_glob: &ScopeGlob,
) -> String {
    format!(
        "`{node}` scopes `{glob}` and `{other}` scopes `{other_glob}`, and the two can reach the \
         same files; they run at once, in one tree"
    )
}

fn unknown_mode_node(mode: &ModeName, node: &NodeId) -> String {
    format!("mode `{mode}` includes `{node}`, and no top-level node carries that id")
}

fn invariant_left_out(mode: &ModeName) -> String {
    format!(
        "this node is `invariant` and mode `{mode}` leaves it out; a mode chooses what to skip, \
         never what the workflow cannot run without"
    )
}

fn reroute_left_out(field: &str, target: &NodeId, mode: &ModeName) -> String {
    format!(
        "`{field}` sends control to `{target}`, and mode `{mode}` leaves `{target}` out; a mode \
         that keeps a node keeps what it reroutes to"
    )
}

fn source_left_out(field: &str, source: &NodeId, mode: &ModeName) -> String {
    format!(
        "`{field}` reads from `{source}`, and mode `{mode}` leaves `{source}` out, as does every \
         mode before it; a mode that keeps a node keeps what it reads from"
    )
}

fn optional_in_parallel(node: &NodeId, group: &NodeId) -> String {
    format!(
        "`{node}` is a child of the `parallel` group `{group}` and declares `optional: true`, \
         which only a top-level node can honor; declare it on the group"
    )
}

fn reroutes_to_optional(target: &NodeId) -> String {
    format!(
        "re-routes to `{target}`, which is optional: a run that leaves it out would have nowhere \
         to go"
    )
}

fn reads_what_may_go(field: &str, source: &NodeId, node: &NodeId) -> String {
    format!(
        "`{field}` reads from `{source}`, which a run may leave out; make `{node}` optional too, \
         or `{source}` required"
    )
}

#[cfg(test)]
mod tests {
    use super::WorkflowFault;
    use crate::{ModeName, NodeId};

    #[test]
    fn each_clause_reads_as_the_workflow_has_always_said_it() {
        assert_eq!(
            WorkflowFault::UnknownReference {
                field: "depends_on".to_string(),
                target: NodeId::from("buld"),
                nearest: Some(NodeId::from("build")),
            }
            .clause(),
            "`depends_on` names `buld`, and no node carries that id — did you mean `build`?"
        );
        assert_eq!(
            WorkflowFault::UnknownReference {
                field: "depends_on".to_string(),
                target: NodeId::from("zeta"),
                nearest: None,
            }
            .clause(),
            "`depends_on` names `zeta`, and no node carries that id"
        );
        assert_eq!(
            WorkflowFault::RerouteLeftOut {
                field: "on_failure.goto".to_string(),
                target: NodeId::from("fix"),
                mode: ModeName::from("quick"),
            }
            .clause(),
            "`on_failure.goto` sends control to `fix`, and mode `quick` leaves `fix` out; a mode \
             that keeps a node keeps what it reroutes to"
        );
        assert_eq!(
            WorkflowFault::ReadsWhatMayGo {
                field: "context: artifact".to_string(),
                source: NodeId::from("plan"),
                node: NodeId::from("build"),
            }
            .clause(),
            "`context: artifact` reads from `plan`, which a run may leave out; make `build` \
             optional too, or `plan` required"
        );
    }
}
