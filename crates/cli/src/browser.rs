//! A page shown in a browser, by the system's own opener: what `yunta open` does for a
//! person at a terminal, and what the application does with a page it is asked to show.
//! One way to a browser, for both.

use std::path::Path;
use std::process::{Command, Stdio};

/// Asks the system's own opener for `page`. The opener hands the page to the
/// browser and is gone, so nothing waits on it.
pub(crate) fn open(page: &Path) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(page)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}
