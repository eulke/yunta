//! A page or a link shown in a browser, by the system's own opener: what `yunta open` does
//! for a person at a terminal, and what the application does with what it is asked to
//! show. The opener takes a page's path and a link alike and hands either to the browser.
//! One way to a browser, for both.

use std::process::{Command, Stdio};

use yunta_ui::Address;

/// Asks the system's own opener for `address`. The opener hands it to the browser and is
/// gone: nothing waits on it, and a thread of its own reaps it once it is, so an
/// application that shows page after page leaves no ended opener behind it.
pub(crate) fn open(address: &Address) -> std::io::Result<()> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut opener = Command::new(program);
    match address {
        Address::Page(page) => opener.arg(page),
        Address::Link(link) => opener.arg(link),
    };
    let mut opening = opener
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
