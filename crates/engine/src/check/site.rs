//! Where in a workflow a reference is written.

use std::fmt;

use yunta_core::workflow::reads::ReadSite;
use yunta_core::yaml::Pointer;
use yunta_core::NodeId;

/// Where in a workflow a reference is written: the place in the file,
/// and how a sentence about it names that place.
///
/// One value, because a diagnostic says the sentence and a surface that
/// quotes the file needs the place, and two loose fields drift into
/// naming two different places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    at: Pointer,
    said: String,
}

impl Site {
    /// `said` of the place `at`.
    pub(crate) fn new(at: Pointer, said: String) -> Self {
        Site { at, said }
    }

    /// The reference `reader` writes under the field `read` is.
    pub(crate) fn read(read: ReadSite, reader: &NodeId) -> Self {
        let key = match read {
            ReadSite::Context => "context",
            ReadSite::Mount => "mounts",
            ReadSite::Shows => "shows",
        };
        Site::new(node(reader).key(key), read.of(reader))
    }

    /// The place in the workflow file, from its root.
    pub fn pointer(&self) -> &Pointer {
        &self.at
    }
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.said)
    }
}

/// Where the node `id` is declared in a workflow file.
pub(crate) fn node(id: &NodeId) -> Pointer {
    Pointer::root().key("nodes").node(id.as_str())
}
