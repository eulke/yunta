//! What a terminal is told that is not color: its cursor, what it erases, how it
//! marks a paste, how it takes one drawing whole, its window's title and its bell.

use super::harmless;

/// Hides the cursor, while a prompt draws and no field is open.
pub const HIDE_CURSOR: &str = "\x1b[?25l";

/// Shows the cursor again.
pub const SHOW_CURSOR: &str = "\x1b[?25h";

/// Erases from the cursor to the end of the screen.
pub const CLEAR_DOWN: &str = "\x1b[J";

/// Puts the terminal on its other screen, which is empty and has no history. What was on
/// this one comes back when it is left.
pub const ALT_SCREEN_ON: &str = "\x1b[?1049h";

/// Comes back from the other screen.
pub const ALT_SCREEN_OFF: &str = "\x1b[?1049l";

/// Has the wheel send arrows on the other screen, without taking the mouse: selecting
/// text goes on working. What the terminal had is saved first, since most have it on and
/// a person's other programs count on that.
pub const ALT_SCROLL_ON: &str = "\x1b[?1007s\x1b[?1007h";

/// Puts back what the wheel did on the other screen.
pub const ALT_SCROLL_OFF: &str = "\x1b[?1007r";

/// Moves the cursor to the first cell of the first row.
pub const HOME: &str = "\x1b[H";

/// Moves the cursor up `rows` rows. Nothing for none: `ESC [ 0 A` moves one.
pub fn up(rows: usize) -> String {
    match rows {
        0 => String::new(),
        rows => format!("\x1b[{rows}A"),
    }
}

/// Moves the cursor right `cells` cells. Nothing for none, for the same reason.
pub fn right(cells: usize) -> String {
    match cells {
        0 => String::new(),
        cells => format!("\x1b[{cells}C"),
    }
}

/// Asks the terminal to mark pasted text (`DECSET 2004`): what arrives
/// between the two markers was pasted rather than typed.
pub const PASTE_ON: &str = "\x1b[?2004h";

/// Withdraws the request, leaving the terminal in the mode it was
/// handed over in.
pub const PASTE_OFF: &str = "\x1b[?2004l";

/// Opens one update of the terminal: what follows reaches the screen together,
/// at [`SYNC_END`]. A terminal that does not know it ignores it.
pub const SYNC_BEGIN: &str = "\x1b[?2026h";
/// Closes the update [`SYNC_BEGIN`] opened.
pub const SYNC_END: &str = "\x1b[?2026l";
/// Asks a terminal to ring.
pub const BELL: &str = "\x07";
/// Gives back the window title [`title_saved`] kept.
pub const TITLE_RESTORED: &str = "\x1b[23;2t";

/// Keeps the window's title, and sets it to `title`, whose control characters
/// turn to spaces: any of them could end the sequence, and a tab between two
/// words still keeps them apart.
pub fn title_saved(title: &str) -> String {
    let title = harmless(title).replace(['\n', '\t'], " ");
    format!("\x1b[22;2t\x1b]2;{title}\x1b\\")
}
