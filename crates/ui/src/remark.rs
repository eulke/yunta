//! See [`crate`]. What the API remarks while it works, said: render's words for each
//! remark, and how a person meets them. Every interface says a remark through here,
//! so a caution reads the same on a terminal's stream and on a screen.

use surco::glyphs::Glyphs;
use yunta_api::Remark;
use yunta_render::ending;

/// How a person meets what a remark says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Something a person can still act on, or should know went wrong.
    Warning,
    /// A word beside the work.
    Note,
    /// How far the work has come, for a person who asked to watch it.
    Progress,
}

/// What a remark says, and how a person meets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaidRemark {
    pub weight: Weight,
    pub words: String,
}

/// What `remark` says, in render's words, drawn with `glyphs`.
pub fn said(remark: &Remark, glyphs: Glyphs) -> SaidRemark {
    let (weight, words) = match remark {
        Remark::Caution(caution) => (Weight::Warning, yunta_render::caution::line(caution)),
        Remark::Estimate(estimation) => (
            Weight::Progress,
            yunta_render::spend::estimation(estimation, glyphs),
        ),
        Remark::StaleLockTaken { pid } => (
            Weight::Warning,
            yunta_render::refusal::stale_lock_taken(*pid),
        ),
        Remark::WaitingForReset { run, reason } => (
            Weight::Progress,
            yunta_render::pause::run_waits(run, reason),
        ),
        Remark::ForgeUnavailable { said } => (
            Weight::Warning,
            yunta_render::escalation::forge_unavailable(said),
        ),
        Remark::SignallingEngine { run, engine } => {
            (Weight::Progress, ending::signalling(run, *engine))
        }
        Remark::StoppedWaiting { run } => (Weight::Note, ending::stopped_waiting(run)),
        Remark::Escalating { run, waited } => (Weight::Note, ending::escalating(run, *waited)),
        Remark::GroupRefused { pgid } => (Weight::Warning, ending::group_refused(*pgid)),
        Remark::Unstopped { said } => (Weight::Warning, said.clone()),
        Remark::RegistryKept { said } => (Weight::Warning, ending::registry_kept(said)),
        Remark::CheckoutKept { run, said } => (Weight::Warning, ending::checkout_kept(run, said)),
        Remark::Uncollected(uncollected) => (Weight::Warning, uncollected.said()),
        Remark::ReplacedTreeKept { at, said } => (
            Weight::Warning,
            yunta_render::refusal::replaced_tree_kept(at, said),
        ),
        Remark::UnreadManifestKeys { keys } => {
            let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
            (
                Weight::Note,
                yunta_render::caution::unread_manifest_keys(&keys),
            )
        }
    };
    SaidRemark {
        weight,
        words: words.to_string(),
    }
}
