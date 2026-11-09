//! How the CLI says what the API says while it works: a caution, and what a
//! run's end could not stop or give back, on stderr for whoever watches; what
//! a cancel waits on as a note there; and the estimate, a wait and a signal
//! as progress lines only a person asked for, each as the API says it.

use surco::look::Look;
use yunta_api::Remark;
use yunta_core::events::PauseReason;
use yunta_core::RunId;
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
            progress(output, look, waiting_line(&run, &reason))
        }
        Remark::ForgeUnavailable { said } => warn(format!(
            "the forge is unavailable: {said}; external gates degrade to the console"
        )),
        Remark::SignallingEngine { run, engine } => {
            progress(output, look, ending::signalling(&run, engine))
        }
        Remark::StoppedWaiting { run } => note(ending::stopped_waiting(&run)),
        Remark::Escalating { run, waited } => note(ending::escalating(&run, waited)),
        Remark::GroupRefused { pgid } => warn(ending::group_refused(pgid)),
        Remark::Unstopped { said } => warn(said),
        Remark::RegistryKept { said } => warn(ending::registry_kept(&said)),
        Remark::CheckoutKept { run, said } => warn(ending::checkout_kept(&run, &said)),
    }
}

/// The line a person reads when a resume waits for a limit's reset: the run's
/// handle, the word it is called by, and the sentence the log recorded for the
/// pause, which names the node and the instant.
fn waiting_line(run: &RunId, reason: &PauseReason) -> String {
    format!(
        "run {}: {}",
        run.handle(),
        yunta_render::pause::said(reason)
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use yunta_core::NodeId;

    use super::*;

    #[test]
    fn the_wait_line_names_the_run_its_word_and_the_node_the_reset_is_for() {
        let run_id = RunId::from_static("run-1");
        let at = chrono::Utc
            .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
            .unwrap();
        let reason = PauseReason::AwaitingReset {
            node: NodeId::from_static("build"),
            at,
        };
        let line = waiting_line(&run_id, &reason);
        assert!(
            line.starts_with(&format!(
                "run {}: waiting: node `build` waits for its limit to reset at ",
                run_id.handle()
            )),
            "{line}"
        );
    }
}
