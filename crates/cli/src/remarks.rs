//! How the CLI says what the API says while it works: a caution, and what a
//! run's end could not stop or give back, on stderr for whoever watches; what
//! a cancel waits on as a note there; and the estimate, a wait and a signal
//! as progress lines only a person asked for, each as the API says it.

use surco::look::Look;
use yunta_api::Remark;
use yunta_render::ending;

use crate::output::{note, progress, warn, Mode};

/// Says `remark` the way `output` asks, with `look`.
pub(crate) fn say(remark: Remark, output: Mode, look: &Look) {
    match remark {
        Remark::Caution(caution) => warn(yunta_render::caution::line(&caution)),
        Remark::Estimate(estimation) => progress(
            output,
            look,
            yunta_render::spend::estimation(&estimation, look.glyphs),
        ),
        Remark::StaleLockTaken { pid } => warn(yunta_render::refusal::stale_lock_taken(pid)),
        Remark::WaitingForReset { run, reason } => {
            progress(output, look, yunta_render::pause::run_waits(&run, &reason))
        }
        Remark::ForgeUnavailable { said } => {
            warn(yunta_render::escalation::forge_unavailable(&said))
        }
        Remark::SignallingEngine { run, engine } => {
            progress(output, look, ending::signalling(&run, engine))
        }
        Remark::StoppedWaiting { run } => note(ending::stopped_waiting(&run)),
        Remark::Escalating { run, waited } => note(ending::escalating(&run, waited)),
        Remark::GroupRefused { pgid } => warn(ending::group_refused(pgid)),
        Remark::Unstopped { said } => warn(said),
        Remark::RegistryKept { said } => warn(ending::registry_kept(&said)),
        Remark::CheckoutKept { run, said } => warn(ending::checkout_kept(&run, &said)),
        Remark::Uncollected(uncollected) => warn(uncollected.said()),
        Remark::ReplacedTreeKept { at, said } => {
            warn(yunta_render::refusal::replaced_tree_kept(&at, &said))
        }
        Remark::UnreadManifestKeys { keys } => {
            let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
            note(yunta_render::caution::unread_manifest_keys(&keys))
        }
    }
}
