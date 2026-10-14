//! Where in a workflow a check diagnostic points, as data a surface words.

use serde::{Deserialize, Serialize};

use super::reads::ReadSite;
use crate::ids::NodeId;
use crate::yaml::Pointer;

/// Where in a workflow a check diagnostic points, as data a surface words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "place", rename_all = "snake_case")]
pub enum Place {
    /// A reference written under one of the node's read fields.
    Read { node: NodeId, read: ReadSite },
    /// The run's tasks document, which a loop reads ahead of its first session.
    LoopTasks { node: NodeId },
    /// An artifact the node's external gate hands over.
    ExternalGate { node: NodeId },
    /// A field that names the node it reads from, as written (`context: node-output`).
    Field { node: NodeId, field: String },
    /// An `on_finish.distill` entry.
    Distill,
}

impl Place {
    /// Where the place is in the workflow file, from its root.
    pub fn pointer(&self) -> Pointer {
        let node = |id: &NodeId| Pointer::root().key("nodes").node(id.as_str());
        match self {
            Place::Read { node: id, read } => node(id).key(match read {
                ReadSite::Context => "context",
                ReadSite::Mount => "mounts",
                ReadSite::Shows => "shows",
            }),
            Place::LoopTasks { node: id } => node(id),
            Place::ExternalGate { node: id } => node(id).key("external"),
            Place::Field { node: id, field } => {
                node(id).key(field.split(':').next().unwrap_or(field.as_str()).trim())
            }
            Place::Distill => Pointer::root().key("on_finish"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Place;
    use crate::ids::NodeId;
    use crate::workflow::reads::ReadSite;
    use crate::yaml::Step;

    fn key(name: &str) -> Step {
        Step::Key(name.to_string())
    }

    fn node(id: &str) -> Step {
        Step::Node(id.to_string())
    }

    /// Each read site points at the key its reference is written under.
    #[test]
    fn each_read_site_points_at_the_key_it_is_written_under() {
        let build = NodeId::from("build");
        for (read, key_name) in [
            (ReadSite::Context, "context"),
            (ReadSite::Mount, "mounts"),
            (ReadSite::Shows, "shows"),
        ] {
            let place = Place::Read {
                node: build.clone(),
                read,
            };
            let steps = [key("nodes"), node("build"), key(key_name)];
            assert_eq!(place.pointer().steps(), steps.as_slice(), "{place:?}");
        }
    }

    /// The other variants point at their node, at a key under it, or at the
    /// root's `on_finish` key for a distill entry.
    #[test]
    fn every_other_variant_points_at_its_node_or_its_key() {
        let build = NodeId::from("build");
        let cases = [
            (
                Place::LoopTasks {
                    node: build.clone(),
                },
                vec![key("nodes"), node("build")],
            ),
            (
                Place::ExternalGate {
                    node: build.clone(),
                },
                vec![key("nodes"), node("build"), key("external")],
            ),
            (
                Place::Field {
                    node: build.clone(),
                    field: "context: artifact".into(),
                },
                vec![key("nodes"), node("build"), key("context")],
            ),
            (Place::Distill, vec![key("on_finish")]),
        ];
        for (place, steps) in cases {
            assert_eq!(place.pointer().steps(), steps.as_slice(), "{place:?}");
        }
    }
}
