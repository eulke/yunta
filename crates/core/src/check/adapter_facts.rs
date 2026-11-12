//! See [`super`]. What a check knows of an adapter this binary builds.

use crate::{Capabilities, EditSandbox};

/// What `check` knows of an adapter this binary builds: what it declares,
/// and the `sandbox` its settings give an `edit` session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterFacts {
    pub capabilities: Capabilities,
    pub edit_sandbox: EditSandbox,
}
