//! The one list a person picks from.
//!
//! Every menu the console surface has is this one: the options a gate
//! offers, the values a `choice` question declares, the two a `boolean`
//! question has. Arrows move through it, typing filters it, Enter takes
//! what is highlighted and Escape leaves without an answer.
//!
//! Each option's position is printed beside it and is its shortcut:
//! typing the number brings it to the top of the list. It is the only
//! shortcut available — an option carries an id and a label and no key,
//! and the ids a gate's own menu is built from (`approve`, `adjust`,
//! `abort`) share their first letter, so an initial is neither
//! declarable nor derivable.
//!
//! What an option carries underneath — on a decision, what choosing it
//! trades off — is read above the list, under the same number the list
//! is picked by. The list itself is one short row per option, because
//! that is the only shape it redraws correctly. It clears what it wrote
//! by adding two counts: the line breaks its own drawing carries, and a
//! correction for a row it takes the terminal to have wrapped, measured
//! against the terminal's width in bytes. An option drawn as a block
//! answers the first count and collides with the second, and every
//! keystroke then clears more rows than the list drew and walks it up
//! over what a person is deciding on. A row kept inside the terminal's
//! width in bytes wraps nowhere and draws no correction, so what is
//! cleared is what was written.
//!
//! The keys are named on a line of their own for the same reason: the
//! filter is typed on the list's own top row, and a legend in front of
//! it would push that row past the terminal's width after a few
//! characters.

use dialoguer::FuzzySelect;

use super::{Answered, Console, NoAnswer, PARKS};
use crate::render::{wrap, LINE_WIDTH};

/// The keys a list answers to, named above every one of them.
const KEYS: &str = "arrows move, type to filter, enter chooses";

/// The cells the list marker takes before an option's row.
const MARKER: usize = 2;

/// The cells between an option's number and its text.
const GAP: usize = 2;

/// One option: what it is, what choosing it means, and what the caller
/// gets back for it.
pub(crate) struct Choice<T> {
    /// The option in one line.
    pub(crate) head: String,
    /// What is read under it, when the option carries something.
    pub(crate) detail: Option<String>,
    pub(crate) value: T,
}

/// Puts `choices` to the person — `verb` says what picking one does —
/// and returns the value behind the one picked.
pub(crate) fn choose<T>(console: &Console, verb: &str, choices: Vec<Choice<T>>) -> Answered<T> {
    let read = read(&choices);
    let rows = rows(&choices, console.width());
    // What is read above the list is whatever the list's own rows have
    // no space for: what an option carries underneath, and a head a row
    // had to cut. An option a row holds whole is read on that row.
    if read
        .iter()
        .zip(&rows)
        .any(|(option, row)| option.trim_start() != row.as_str())
    {
        for option in &read {
            console.say(option)?;
        }
        console.say("")?;
    }
    console.say(&format!("{KEYS}, {PARKS}"))?;
    let cursor = Cursor::taken(console);
    let picked = FuzzySelect::new()
        .with_prompt(verb)
        .items(&rows)
        // Without a highlighted option to start from, the first Enter
        // answers nothing.
        .default(0)
        // What was picked is said by whoever asked, in one line, rather
        // than by echoing an option back.
        .report(false)
        .interact_on_opt(console.term())
        .map_err(|failed| NoAnswer::from(std::io::Error::from(failed)))?
        .ok_or(NoAnswer::Declined)?;
    drop(cursor);
    choices
        .into_iter()
        .nth(picked)
        .map(|choice| choice.value)
        .ok_or(NoAnswer::OffMenu)
}

/// Every option as it is read: its position, its text and what it
/// carries underneath, each broken to the cells a line has and every
/// line after the first sitting under the text of the first.
fn read<T>(choices: &[Choice<T>]) -> Vec<String> {
    let digits = digits(choices);
    let indent = " ".repeat(MARKER + digits + GAP);
    let room = LINE_WIDTH.saturating_sub(indent.len());
    let marker = " ".repeat(MARKER);
    choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let mut lines: Vec<String> = Vec::new();
            let text = wrap(&choice.head, room)
                .into_iter()
                .chain(choice.detail.iter().flat_map(|detail| wrap(detail, room)));
            for line in text {
                match lines.is_empty() {
                    true => lines.push(format!(
                        "{marker}{:>digits$}{}{line}",
                        index + 1,
                        " ".repeat(GAP)
                    )),
                    false => lines.push(format!("{indent}{line}")),
                }
            }
            lines.join("\n")
        })
        .collect()
}

/// Every option as the list holds it: its position and its text on one
/// row, inside `width` bytes.
///
/// Cut by bytes and not by cells because that is what the list measures
/// a row it has to clear by; what a cut leaves out is on the line above
/// the list, whole, under the same number.
fn rows<T>(choices: &[Choice<T>], width: usize) -> Vec<String> {
    let digits = digits(choices);
    choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            within(
                &format!("{:>digits$}{}{}", index + 1, " ".repeat(GAP), choice.head),
                width.saturating_sub(MARKER + 1),
            )
        })
        .collect()
}

/// The places a position takes once every option has one.
fn digits<T>(choices: &[Choice<T>]) -> usize {
    choices.len().to_string().len()
}

/// `line` up to the last whole character that keeps it inside `bytes`.
fn within(line: &str, bytes: usize) -> String {
    let mut kept = String::new();
    for character in line.chars() {
        if kept.len() + character.len_utf8() > bytes {
            break;
        }
        kept.push(character);
    }
    kept
}

/// The cursor, put back after the list has drawn over it.
///
/// The list hides the cursor while it draws and shows it again only
/// where it is answered or declined. A prompt that ends any other way —
/// a person stopping the run, a console that stopped reading — would
/// hand the shell it returns to a terminal with no cursor in it, which
/// nothing that runs next puts back.
struct Cursor<'a> {
    console: &'a Console,
}

impl<'a> Cursor<'a> {
    /// Records that the list is about to hide `console`'s cursor.
    fn taken(console: &'a Console) -> Self {
        console.hiding();
        Self { console }
    }
}

impl Drop for Cursor<'_> {
    fn drop(&mut self) {
        self.console.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cells an option's text has once the marker, the position and
    /// the gap after it are drawn.
    const ROOM: usize = LINE_WIDTH - MARKER - 1 - GAP;

    fn choices(details: bool) -> Vec<Choice<u8>> {
        ["approve", "adjust", "abort"]
            .iter()
            .enumerate()
            .map(|(index, head)| Choice {
                head: (*head).to_string(),
                detail: details.then(|| format!("what {head} costs")),
                value: index as u8,
            })
            .collect()
    }

    #[test]
    fn every_option_is_numbered_from_one() {
        assert_eq!(
            read(&choices(false)),
            vec!["  1  approve", "  2  adjust", "  3  abort"]
        );
        assert_eq!(
            rows(&choices(false), LINE_WIDTH),
            vec!["1  approve", "2  adjust", "3  abort"]
        );
    }

    #[test]
    fn an_option_too_long_for_one_line_is_broken_rather_than_left_to_the_terminal() {
        // A tradeoff the engine writes for an exhausted re-route, which
        // is half again as wide as a line has room for.
        let block = read(&[Choice {
            head: "retry — Re-route to `fix` once more".to_string(),
            detail: Some(
                "tradeoff: Uses one extra correction attempt beyond the declared \
                 max_reroutes (0); escalates again if `fix` doesn't fix it"
                    .to_string(),
            ),
            value: 0u8,
        }]);
        let block = block.first().map(String::as_str).unwrap_or_default();
        assert!(
            block.lines().count() > 2,
            "the tradeoff did not break: {block:?}"
        );
        for (at, line) in block.lines().enumerate() {
            let drawn = crate::render::cell_width(line);
            assert!(
                drawn <= LINE_WIDTH,
                "line {at} takes {drawn} cells: {line:?}"
            );
        }
    }

    #[test]
    fn a_second_line_sits_under_the_first_and_not_deeper() {
        let read = read(&choices(true));
        let first = read.first().map(String::as_str).unwrap_or_default();
        let (head, detail) = first.split_once('\n').unwrap_or_default();
        assert_eq!(head, "  1  approve");
        assert_eq!(detail, "     what approve costs");
        assert_eq!(
            detail.len() - detail.trim_start().len(),
            head.find("approve").unwrap_or_default(),
            "what an option carries sits under the option's own text"
        );
    }

    #[test]
    fn a_row_of_the_list_never_takes_more_bytes_than_the_terminal_is_wide() {
        // The list corrects the rows it clears for a row it takes the
        // terminal to have wrapped, and measures that in bytes. A row
        // inside the terminal's width in bytes wraps nowhere and draws
        // no correction, so what it clears is what it wrote.
        const TERMINAL: usize = 40;
        let rows = rows(
            &[Choice {
                head: format!("retry — {}", "é".repeat(ROOM)),
                detail: Some("tradeoff: no room for this on a row".to_string()),
                value: 0u8,
            }],
            TERMINAL,
        );
        for row in &rows {
            assert!(
                row.len() + MARKER < TERMINAL,
                "{} bytes on a terminal {TERMINAL} wide: {row:?}",
                row.len()
            );
            assert!(
                !row.contains('\n'),
                "a row the list counts as one is one: {row:?}"
            );
        }
    }

    #[test]
    fn what_a_row_leaves_out_is_read_whole_above_the_list() {
        let long = Choice {
            head: "retry — Re-route to `fix` once more".to_string(),
            detail: Some("tradeoff: one more attempt".to_string()),
            value: 0u8,
        };
        let rows = rows(std::slice::from_ref(&long), 20);
        let read = read(std::slice::from_ref(&long));
        let row = rows.first().map(String::as_str).unwrap_or_default();
        let block = read.first().map(String::as_str).unwrap_or_default();
        assert!(row.len() < 20 - MARKER, "{row:?}");
        assert!(block.contains(&long.head), "{block:?}");
        assert!(
            block.contains("tradeoff: one more attempt"),
            "what the row had no space for is read above it: {block:?}"
        );
    }
}
