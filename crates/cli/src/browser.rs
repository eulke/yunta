//! A page shown in a browser, by the system's own opener: what `yunta open` does for a
//! person at a terminal, and what the application does with a page it is asked to show.
//! One way to a browser, for both.

use std::path::Path;
use std::process::{Command, Stdio};

/// Asks the system's own opener for `page`. The opener hands the page to the browser and
/// is gone: nothing waits on it, and a thread of its own reaps it once it is, so an
/// application that shows page after page leaves no ended opener behind it.
pub(crate) fn open(page: &Path) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut opening = Command::new(opener)
        .arg(page)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        // best-effort: how the opener ended is the opener's to say, as it said it to a person.
        let _ = opening.wait();
    });
    Ok(())
}
