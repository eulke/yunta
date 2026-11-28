//! See [`super`]. The link `o` hands the surface to show the pull request by: one the log
//! holds is handed only where it is `https://`, since the system's opener launches what any
//! other scheme names.

use surco::words::Standing;
use yunta_core::events::PauseReason;
use yunta_core::RunId;
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};

use super::link;

/// The run every test frames.
const RUN: RunId = RunId::from_static("01M7PR00000000000000000ABC");

/// The run parked on a gate a forge holds, whose pull request the log links at `url`.
fn waiting_at(url: &str) -> RunFrame {
    let reason = PauseReason::ExternalGate {
        url: url.to_string(),
    };
    RunFrame {
        phase: RunPhase::Waiting {
            on: WaitingOn::Run { reason },
        },
        standing: Standing::NeedsYou,
        ..RunFrame::sample(&RUN)
    }
}

/// A pull request linked `https://` is handed to show; one the log links to a file, which
/// the opener would launch, is not.
#[test]
fn a_link_is_handed_to_show_only_where_it_is_https() {
    let forge = waiting_at("https://github.com/acme/web/pull/7");
    assert!(link(&forge).is_some(), "a forge's page is shown");
    let app = waiting_at("file:///System/Applications/Calculator.app");
    assert!(link(&app).is_none(), "a file is never handed to the opener");
}
