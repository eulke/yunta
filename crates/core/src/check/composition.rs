//! See [`super`]. A workflow another one composes, and what makes the
//! reference to it unusable.

use std::path::PathBuf;

use super::CheckError;
use crate::NodeId;

/// What refuses a `use:` reference: one that does not resolve, or
/// resolves to a workflow its birth would refuse, or composes into a
/// cycle or past the depth a run allows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Composition {
    /// A composition reference that can't resolve today — the
    /// same broken-reference class as `BrokenReference`, across
    /// files. Advisory about the *current* catalog by design: the child
    /// freezes its own file at birth, so a run only ever meets the file
    /// as it is then.
    #[error("composition_missing")]
    Missing {
        node: NodeId,
        name: String,
        detail: String,
    },

    /// Two packs installed under the same publisher each declare a
    /// workflow with the same file basename — the flat
    /// `publisher/workflow` namespace can't tell them apart.
    #[error("composition_ambiguous")]
    Ambiguous {
        node: NodeId,
        name: String,
        detail: String,
    },

    /// Cross-pack references aren't supported — a workflow
    /// that lives inside a pack may only `use:` other workflows from
    /// that same pack, never the repo's own catalog or a different
    /// pack (no transitive pack dependencies).
    #[error("composition_cross_pack")]
    CrossPack {
        node: NodeId,
        name: String,
        from_pack: String,
    },

    #[error("composition_unparseable")]
    Unparseable {
        path: PathBuf,
        report: Box<crate::diagnostic::Report>,
    },

    /// A workflow this one composes that its birth would refuse, found
    /// before the run that would compose it spends anything.
    #[error("composition_fails")]
    Fails {
        node: NodeId,
        name: String,
        problems: Vec<CheckError>,
    },

    /// The graph of references between workflows must be acyclic.
    #[error("composition_cycle")]
    Cycle { chain: String },

    /// The configurable maximum nesting depth, checked statically over
    /// the reference graph (the runtime guard at child birth enforces
    /// the same limit over what actually loads).
    #[error("composition_too_deep")]
    TooDeep { chain: String, depth: u32, max: u32 },
}
