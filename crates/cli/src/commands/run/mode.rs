//! The mode a run starts in, resolved the same way for every shape a run
//! takes: the `--mode` a person gave, or else the workflow's first
//! declared mode.

use yunta_core::{Manifest, ModeName};

/// The mode a run of `manifest` runs in. An explicit `--mode` is used as
/// given (`create_run` itself refuses an unknown name); omitted with
/// `modes:` declared defaults to the *first* declared mode — promotion
/// only ever escalates forward, so starting at the floor is the one
/// default that can never need walking back. A workflow with no `modes:`
/// at all keeps running everything, unaffected.
pub(super) fn resolve_mode(manifest: &Manifest, mode: Option<&ModeName>) -> ModeName {
    mode.cloned().unwrap_or_else(|| {
        manifest
            .workflow
            .modes
            .as_ref()
            .and_then(|modes| modes.keys().next())
            .cloned()
            .unwrap_or_default()
    })
}
