//! A prompt that stays where it opened and redraws over itself as keys arrive.
//!
//! One loop for every such prompt, because the terminal is one terminal: for as
//! long as a prompt holds it nothing typed is echoed, what is pasted is marked, the
//! cursor shows only on a field that is open, and each drawing goes out whole. When
//! the prompt ends, by any path, what it drew is off the screen and the terminal is
//! as it was found.

use nix::sys::termios::{tcgetattr, tcsetattr, LocalFlags, SetArg, Termios};
use surco::look::Look;
use surco::paint::{right, up, CLEAR_DOWN, PASTE_OFF, PASTE_ON, SYNC_BEGIN, SYNC_END};
use surco::surface::Placed;
use surco::width::cell_width;

use super::keys::{Stroke, Strokes};
use super::{Answered, Console, NoAnswer};
use crate::error::warn;

/// A prompt the loop can hold.
pub(crate) trait Held {
    /// What answering it comes to.
    type Answer;

    /// What it draws, in `rows` rows of the terminal.
    fn drawn(&self, look: &Look, rows: usize) -> Placed;

    /// What a stroke asks of it.
    fn press(&mut self, stroke: Stroke) -> Press<Self::Answer>;

    /// Whether a field is open, where what is pasted is what is typed. Anywhere
    /// else a paste answers nothing.
    fn typing(&self) -> bool;
}

/// What a stroke asks of a prompt.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Press<T> {
    /// It stays, drawn again.
    Stay,
    /// It stays, on another question: what was typed for the last one is no answer
    /// to this one.
    Moved,
    /// It is answered.
    Answer(T),
    /// The person leaves it without an answer.
    Decline,
    /// The person stops the run.
    Interrupt,
}

/// What ended a prompt.
enum Ended<T> {
    Answer(T),
    Decline,
    Interrupt,
}

/// Puts `prompt` to the person until it is answered.
pub(crate) fn hold<P: Held>(console: &Console, prompt: &mut P) -> Answered<P::Answer> {
    console.discard_typed();
    let quiet = Quiet::on(console)?;
    let mut place = Place::default();
    let mut strokes = Strokes::default();
    let ended = hold_until_it_ends(console, prompt, &mut place, &mut strokes);
    let erased = place.erase(console);
    drop(quiet);
    match ended? {
        Ended::Answer(answer) => erased.map(|()| answer).map_err(NoAnswer::from),
        Ended::Decline => Err(NoAnswer::Declined),
        // Only now, with the prompt off the screen, so what the run says about being
        // stopped starts where the prompt stood.
        Ended::Interrupt => {
            console.interrupt();
            Err(NoAnswer::Interrupted)
        }
    }
}

/// Reads keys and applies them to `prompt` until a stroke ends it, or the console is
/// stopped. A stroke that stays redraws it, and so does a tick that finds the terminal
/// changed size. A stroke that moves to another question discards what was typed
/// before it is drawn.
fn hold_until_it_ends<P: Held>(
    console: &Console,
    prompt: &mut P,
    place: &mut Place,
    strokes: &mut Strokes,
) -> Answered<Ended<P::Answer>> {
    redraw(console, prompt, place)?;
    loop {
        let Some(key) = console.next_key()? else {
            if console.size() != place.size {
                redraw(console, prompt, place)?;
            }
            continue;
        };
        let stroke = strokes.read(key);
        if strokes.pasting() && !prompt.typing() {
            continue;
        }
        match prompt.press(stroke) {
            Press::Stay => redraw(console, prompt, place)?,
            Press::Moved => {
                console.discard_typed();
                redraw(console, prompt, place)?;
            }
            Press::Answer(answer) => return Ok(Ended::Answer(answer)),
            Press::Decline => return Ok(Ended::Decline),
            Press::Interrupt => return Ok(Ended::Interrupt),
        }
    }
}

/// Draws `prompt` over what `place` last drew, at the terminal's size now.
fn redraw<P: Held>(console: &Console, prompt: &P, place: &mut Place) -> std::io::Result<()> {
    let look = console.look();
    let (_, rows) = console.size();
    place.draw(console, &prompt.drawn(&look, rows), &look)
}

/// The terminal as a prompt holds it: each key reaches the read as it is pressed
/// rather than at the end of a line, nothing typed is echoed between one read and the
/// next, and a paste is marked. Put back on every path out, with a cursor a prompt hid.
struct Quiet<'a> {
    console: &'a Console,
    /// The mode it was found in. `None` where the terminal will not say, which
    /// leaves it as it is.
    found: Option<Termios>,
}

impl<'a> Quiet<'a> {
    /// Takes the terminal as a prompt holds it. The signal keys go off, so a typed
    /// Ctrl-C is a key this prompt reads and comes off the screen for before the run
    /// hears it stopped, and output processing stays on, so a line still ends where
    /// it should.
    /// The guard is built before anything is changed, so a failure on the way in
    /// returns through its drop, which puts back whatever was changed by then.
    fn on(console: &'a Console) -> std::io::Result<Self> {
        let guard = Self {
            console,
            found: tcgetattr(std::io::stdin()).ok(),
        };
        if let Some(mode) = &guard.found {
            let mut quiet = mode.clone();
            quiet
                .local_flags
                .remove(LocalFlags::ECHO | LocalFlags::ICANON | LocalFlags::ISIG);
            tcsetattr(std::io::stdin(), SetArg::TCSANOW, &quiet).map_err(std::io::Error::from)?;
        }
        console.term().write_str(PASTE_ON)?;
        Ok(guard)
    }
}

impl Drop for Quiet<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.console.term().write_str(PASTE_OFF) {
            warn(format!(
                "could not restore the terminal's paste marking: {e}"
            ));
        }
        if let Err(e) = self.console.show_cursor() {
            warn(format!(
                "could not put the terminal's cursor back: {e}; run `reset` if the cursor stays hidden"
            ));
        }
        if let Some(mode) = &self.found {
            if let Err(e) = tcsetattr(std::io::stdin(), SetArg::TCSANOW, mode) {
                warn(format!(
                    "could not put this terminal back to reading a line at a time: {e}; \
                     run `stty sane` to type into your shell again"
                ));
            }
        }
    }
}

/// What a prompt last drew: the cells each line takes, the row the cursor was left
/// on, and the size of the terminal it was drawn on.
#[derive(Default)]
struct Place {
    /// The cells each line that is drawn takes, in order.
    widths: Vec<usize>,
    /// How many rows the cursor stands below the first row drawn.
    below_top: usize,
    /// The columns and the rows of the terminal it was drawn on.
    size: (usize, usize),
}

impl Place {
    /// Draws `placed` over what was drawn before, in one write, and leaves the
    /// cursor where the prompt puts it: on its field when it has one, and after the
    /// last line when it has not.
    fn draw(&mut self, console: &Console, placed: &Placed, look: &Look) -> std::io::Result<()> {
        let (columns, rows) = console.size();
        let climb = self.climb(columns);
        let widths: Vec<usize> = placed
            .lines
            .iter()
            .map(|line| cell_width(&line.text()))
            .collect();
        let skip = dropped(&widths, columns, rows.saturating_sub(1));
        let kept = widths.get(skip..).unwrap_or_default();
        let cursor = placed
            .cursor
            .and_then(|(row, cell)| row.checked_sub(skip).map(|row| (row, cell)));
        let mut out = String::from(SYNC_BEGIN);
        out.push('\r');
        out.push_str(&up(climb));
        out.push_str(CLEAR_DOWN);
        out.push_str(&lines_out(
            placed.lines.get(skip..).unwrap_or_default(),
            look,
        ));
        let end = rows_of(kept, columns).saturating_sub(1);
        let below_top = match cursor {
            Some((row, cell)) => {
                let start = rows_above(kept, columns, row).min(end);
                out.push_str(&up(end - start));
                out.push('\r');
                out.push_str(&right(cell));
                start
            }
            None => end,
        };
        out.push_str(SYNC_END);
        console.term().write_str(&out)?;
        console.term().flush()?;
        self.below_top = below_top;
        self.widths = kept.to_vec();
        self.size = (columns, rows);
        match cursor {
            Some(_) => console.show_cursor(),
            None => console.hide_cursor(),
        }
    }

    /// Takes what was drawn off the screen, leaving the cursor where the first row
    /// drawn was. Nothing is written when nothing was drawn.
    fn erase(&self, console: &Console) -> std::io::Result<()> {
        if self.widths.is_empty() {
            return Ok(());
        }
        let (columns, _) = console.size();
        let mut out = String::from("\r");
        out.push_str(&up(self.climb(columns)));
        out.push_str(CLEAR_DOWN);
        console.term().write_str(&out)?;
        console.term().flush()
    }

    /// How many rows the cursor stands below the top of what was drawn, on a
    /// terminal `columns` wide. A terminal that was resized since the last draw has
    /// rewrapped what was drawn, and the cursor is taken to be on its last row.
    fn climb(&self, columns: usize) -> usize {
        if columns == self.size.0 {
            self.below_top
        } else {
            rows_of(&self.widths, columns).saturating_sub(1)
        }
    }
}

/// The lines as they are written: each painted, one after another, with a carriage
/// return and a line feed between them. The return is written here rather than left
/// to the terminal's output processing, so a line starts at the left edge whether or
/// not that processing is on.
fn lines_out(lines: &[surco::line::Line], look: &Look) -> String {
    lines
        .iter()
        .map(|line| look.paint.line(line))
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// The terminal rows a line `width` cells wide takes on a terminal `columns` wide. An
/// empty line still takes one row.
fn physical(width: usize, columns: usize) -> usize {
    width.max(1).div_ceil(columns.max(1))
}

/// The terminal rows the lines of `widths` take on a terminal `columns` wide.
fn rows_of(widths: &[usize], columns: usize) -> usize {
    widths.iter().map(|width| physical(*width, columns)).sum()
}

/// The terminal rows the lines before `line` take, which is how far below the top
/// of the drawing `line` starts.
fn rows_above(widths: &[usize], columns: usize, line: usize) -> usize {
    widths
        .iter()
        .take(line)
        .map(|width| physical(*width, columns))
        .sum()
}

/// How many leading lines are left out so the rest fit in `most` rows. A terminal
/// cannot be drawn on above its first row, so a prompt taller than the terminal keeps
/// its last lines, the ones with the field and the keys.
fn dropped(widths: &[usize], columns: usize, most: usize) -> usize {
    let mut used = 0;
    let mut kept_from = widths.len();
    for (at, width) in widths.iter().enumerate().rev() {
        let taken = physical(*width, columns);
        if used + taken > most {
            break;
        }
        used += taken;
        kept_from = at;
    }
    kept_from
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_takes_one_row_until_it_is_wider_than_the_terminal() {
        assert_eq!(physical(80, 80), 1);
        assert_eq!(physical(81, 80), 2);
        assert_eq!(physical(0, 80), 1);
    }

    #[test]
    fn a_prompt_taller_than_the_terminal_keeps_its_last_lines() {
        assert_eq!(dropped(&[80, 80, 80], 80, 2), 1);
        assert_eq!(dropped(&[10, 10], 80, 5), 0);
        assert_eq!(dropped(&[10, 10], 80, 0), 2);
    }

    #[test]
    fn a_cursor_row_is_counted_from_the_first_row_of_its_line() {
        assert_eq!(rows_above(&[80, 81, 10], 80, 2), 3);
        assert_eq!(rows_of(&[80, 81, 10], 80), 4);
    }
}
