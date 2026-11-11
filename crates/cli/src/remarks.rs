//! How the CLI says what the API says while it works: a caution, and what a
//! run's end could not stop or give back, on stderr for whoever watches; what
//! a cancel waits on as a note there; and the estimate, a wait and a signal
//! as progress lines only a person asked for, each as the API says it.

use surco::look::Look;
use yunta_api::Remark;
use yunta_ui::remark::Weight;

use crate::output::{note, progress, warn, Mode};

/// Says `remark` the way `output` asks, with `look`: in the words and with the weight
/// every interface says it with.
pub(crate) fn say(remark: Remark, output: Mode, look: &Look) {
    let said = yunta_ui::remark::said(&remark, look.glyphs);
    match said.weight {
        Weight::Warning => warn(said.words),
        Weight::Note => note(said.words),
        Weight::Progress => progress(output, look, said.words),
    }
}
