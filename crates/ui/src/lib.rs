//! Yunta's user interface, apart from the medium that shows it: what a person
//! has open, what each stroke and each piece of news does to it, the work it
//! asks of Yunta as data, and each moment as a Surco `Screen`. The terminal
//! application and the desktop are surfaces over it: each reads its own input
//! and hands it in, carries out the work it asks for, and lays out the screen
//! it returns, deciding nothing the other would decide again.
//!
//! It writes no sentence, which is `yunta-render`'s, and names no terminal,
//! no window, no engine, no store and no adapter: every operation goes
//! through `yunta-api`. `tests/no_terminal_in_the_interface.rs` holds the
//! second half.
//!
//! Its map: `prompt`, the prompts a person answers stroke by stroke, each the
//! [`Answerable`](prompt::Answerable) a surface holds, the [`Stroke`](prompt::Stroke)
//! it is handed and the [`Press`](prompt::Press) it answers with.

pub mod prompt;
