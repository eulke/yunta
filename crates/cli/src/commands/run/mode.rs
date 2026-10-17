//! The mode a run starts in, resolved the same way for every shape a run
//! takes: the `--mode` a person gave, or else the workflow's first
//! declared mode.

use yunta_core::{Manifest, ModeName};

/// The mode a run of `manifest` runs in. An explicit `--mode` is used as
/// given (`create_run` itself refuses an unknown name); omitted, it is the
/// workflow's first mode, the one a child run starts at too.
pub(super) fn resolve_mode(manifest: &Manifest, mode: Option<&ModeName>) -> ModeName {
    mode.cloned()
        .unwrap_or_else(|| manifest.workflow.first_mode())
}
