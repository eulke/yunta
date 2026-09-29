//! What a node reads by reference: the artifacts it names and the nodes
//! it reads from.
//!
//! A node names an artifact in three places — a `context:` source, a
//! `mounts:` entry, a gate's `shows:` — and every rule about such a
//! reference holds for all three: it orders the node behind the node it
//! names, a mode that keeps the reader keeps what it reads, and the
//! named node declares what is asked of it. One list of them is what
//! keeps a fourth place from reaching one rule and not the others.

use super::{ArtifactRefId, ContextSpec, Node, NodeKind};
use crate::ids::NodeId;

/// Where a node names an artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadSite {
    Context,
    Mount,
    Shows,
}

impl ReadSite {
    /// The field the reference is written under.
    pub fn field(self) -> &'static str {
        match self {
            ReadSite::Context => "context: artifact",
            ReadSite::Mount => "mounts",
            ReadSite::Shows => "shows",
        }
    }

    /// Where the reference sits, as a diagnostic about `reader` names it.
    pub fn of(self, reader: &NodeId) -> String {
        match self {
            ReadSite::Context => format!("the `artifact:` context source of node `{reader}`"),
            ReadSite::Mount => format!("a `mounts:` entry of node `{reader}`"),
            ReadSite::Shows => format!("the `shows:` of gate `{reader}`"),
        }
    }
}

/// One artifact a node names: where, the node it names it of — `None`
/// for an artifact of the run, whoever produced it — and which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactRead<'a> {
    pub site: ReadSite,
    pub node: Option<&'a NodeId>,
    pub id: &'a ArtifactRefId,
}

/// Every artifact `node` names, in the order it names them. A
/// `parallel` group's children name their own.
pub fn artifact_reads(node: &Node) -> Vec<ArtifactRead<'_>> {
    let mut reads: Vec<ArtifactRead<'_>> = node
        .context
        .iter()
        .filter_map(|source| match source {
            ContextSpec::Artifact { artifact } => Some(ArtifactRead {
                site: ReadSite::Context,
                node: artifact.node.as_ref(),
                id: &artifact.id,
            }),
            _ => None,
        })
        .collect();
    match &node.kind {
        NodeKind::Workflow { mounts, .. } => {
            reads.extend(mounts.iter().map(|mount| ArtifactRead {
                site: ReadSite::Mount,
                node: Some(&mount.artifact.node),
                id: &mount.artifact.id,
            }));
        }
        NodeKind::Gate { shows, .. } => {
            reads.extend(shows.iter().map(|shown| ArtifactRead {
                site: ReadSite::Shows,
                node: shown.node.as_ref(),
                id: &shown.id,
            }));
        }
        _ => {}
    }
    reads
}

/// The nodes a node reads from by name — a named artifact, a captured
/// output — its `parallel` children's included, since a group is kept
/// or left out whole. Each with the field it is named in.
pub fn read_sources(node: &Node) -> Vec<(&'static str, &NodeId)> {
    let mut sources: Vec<(&'static str, &NodeId)> = artifact_reads(node)
        .into_iter()
        .filter_map(|read| read.node.map(|named| (read.site.field(), named)))
        .collect();
    for source in &node.context {
        if let ContextSpec::NodeOutput { node_output } = source {
            sources.push(("context: node-output", &node_output.node));
        }
    }
    if let NodeKind::Parallel { nodes, .. } = &node.kind {
        sources.extend(nodes.iter().flat_map(read_sources));
    }
    sources
}
