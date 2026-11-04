//! Code a plan writes, read against the code that stands where it goes
//! today, word by word: the words it keeps, the ones it puts in and the
//! ones it takes out, each laid out where a reader looks for it on the
//! plan's own lines.
//!
//! The two are read as words of their file's language, so a comment, a
//! line break or a comma where a formatter puts one is never a change. The
//! words both say are the longest run of them the two share, in order; a
//! change that only puts words in, or only takes them out, stands where it
//! reads best among the places that say the same; and a short stretch kept
//! between two changes on one line reads as part of one change with them.

use std::ops::Range;

use super::lex::Language;
use crate::components::{Against, Marks, Part};

mod read;

use read::{collapsed, Read};

/// The most cells the table that compares two codes word by word holds —
/// a thousand words against a thousand — before the two are said to differ
/// without their marks: the table grows with the product of their lengths.
const COMPARED: usize = 1 << 20;

/// `planned`, the code a plan writes in the file at `at`, read against
/// `today`: unchanged when their words are the same, else modified, each of
/// `planned`'s lines as the parts it reads in when the two are short enough
/// to compare word by word.
pub(crate) fn compared(at: &str, planned: &str, today: &str) -> Against {
    let language = Language::of(at);
    let planned = Read::of(planned, language);
    let today = Read::of(today, language);
    if planned.said().eq(today.said()) {
        return Against::Unchanged;
    }
    let Some(runs) = aligned(&planned, &today) else {
        return Against::Modified(None);
    };
    let runs = cleaned(slid(runs, &planned, &today), &planned, &today);
    Against::Modified(Some(laid_out(&runs, &planned, &today)))
}

/// A stretch of the two codes lined up: the planned code's words in it and
/// today's, which say the same one for one where it is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    planned: Range<usize>,
    today: Range<usize>,
    kept: bool,
}

/// What one word of the two codes is to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Both say it.
    Kept,
    /// Only the planned code says it.
    Added,
    /// Only today's code says it.
    Removed,
}

/// The two codes' words lined up: the longest run of words both say kept,
/// in order, and what stands between them changed. The runs go from a kept
/// one, which may hold no words, to another, a change between each two.
/// `None` when what differs between the words both open and close with is
/// too long to line up.
fn aligned(planned: &Read, today: &Read) -> Option<Vec<Run>> {
    let (p, t) = (planned.words.len(), today.words.len());
    let head = (0..p.min(t))
        .take_while(|&at| planned.word(at) == today.word(at))
        .count();
    let tail = (0..(p - head).min(t - head))
        .take_while(|&back| planned.word(p - 1 - back) == today.word(t - 1 - back))
        .count();
    let middle = (head..p - tail, head..t - tail);
    if middle.0.len().saturating_mul(middle.1.len()) > COMPARED {
        return None;
    }
    let mut runs = vec![Run {
        planned: 0..head,
        today: 0..head,
        kept: true,
    }];
    let mut at = (head, head);
    for step in steps(planned, today, middle) {
        let next = (
            at.0 + usize::from(step != Step::Removed),
            at.1 + usize::from(step != Step::Added),
        );
        joined(&mut runs, at.0..next.0, at.1..next.1, step == Step::Kept);
        at = next;
    }
    joined(&mut runs, at.0..p, at.1..t, true);
    Some(runs)
}

/// `runs` with the stretch of words `planned` and `today` after them: part
/// of the last run when it is of the same kind, a run of its own otherwise.
fn joined(runs: &mut Vec<Run>, planned: Range<usize>, today: Range<usize>, kept: bool) {
    match runs.last_mut() {
        Some(last) if last.kept == kept => {
            last.planned.end = planned.end;
            last.today.end = today.end;
        }
        _ => runs.push(Run {
            planned,
            today,
            kept,
        }),
    }
}

/// What each word of the planned code's words `rows` and today's `columns`
/// is to the other, in order, read from the longest run of words both say.
fn steps(planned: &Read, today: &Read, (rows, columns): (Range<usize>, Range<usize>)) -> Vec<Step> {
    let width = columns.len() + 1;
    let same = |row: usize, column: usize| {
        planned.word(rows.start + row) == today.word(columns.start + column)
    };
    // How many words the two say alike, in order, from a row and a column
    // on; there is nothing to count where either says no word.
    let cells = match rows.is_empty() || columns.is_empty() {
        true => 0,
        false => (rows.len() + 1) * width,
    };
    let mut longest = vec![0u32; cells];
    for row in (0..rows.len()).rev() {
        for column in (0..columns.len()).rev() {
            longest[row * width + column] = match same(row, column) {
                true => longest[(row + 1) * width + column + 1] + 1,
                false => longest[(row + 1) * width + column].max(longest[row * width + column + 1]),
            };
        }
    }
    let (mut row, mut column) = (0, 0);
    let mut steps = Vec::with_capacity(rows.len() + columns.len());
    while row < rows.len() || column < columns.len() {
        let step = match (row < rows.len(), column < columns.len()) {
            (true, true) if same(row, column) => Step::Kept,
            (true, true)
                if longest[(row + 1) * width + column] >= longest[row * width + column + 1] =>
            {
                Step::Added
            }
            (true, false) => Step::Added,
            _ => Step::Removed,
        };
        row += usize::from(step != Step::Removed);
        column += usize::from(step != Step::Added);
        steps.push(step);
    }
    steps
}

/// `runs` with each change that only puts words in, or only takes words
/// out, moved among the places that say the same to the one it reads best
/// at.
fn slid(mut runs: Vec<Run>, planned: &Read, today: &Read) -> Vec<Run> {
    for at in 0..runs.len() {
        let run = &runs[at];
        let (read, words) = match (run.kept, run.planned.is_empty(), run.today.is_empty()) {
            (false, false, true) => (planned, run.planned.clone()),
            (false, true, false) => (today, run.today.clone()),
            _ => continue,
        };
        let to = best_place(&runs, at, read, &words);
        shift(&mut runs, at, words.start, to);
    }
    runs
}

/// Where the change at `at`, words `words` of `read`, starts at its best:
/// among the places it reaches moving past the words the kept runs beside
/// it say alike at its two ends, the one it fits best, the rightmost of
/// two that fit as well.
fn best_place(runs: &[Run], at: usize, read: &Read, words: &Range<usize>) -> usize {
    let back = (1..=room(runs, at.checked_sub(1)))
        .take_while(|&by| read.word(words.start - by) == read.word(words.end - by))
        .count();
    let ahead = (0..room(runs, Some(at + 1)))
        .take_while(|&by| read.word(words.end + by) == read.word(words.start + by))
        .count();
    (words.start - back..=words.start + ahead)
        .max_by_key(|&start| (fit(read, start..start + words.len()), start))
        .unwrap_or(words.start)
}

/// How many words the kept run at `at` can give the change beside it: all
/// of them at either end of the code, all but one between two changes, so
/// the two stay apart.
fn room(runs: &[Run], at: Option<usize>) -> usize {
    let Some(run) = at.and_then(|at| runs.get(at)).filter(|run| run.kept) else {
        return 0;
    };
    let edge = at == Some(0) || at == Some(runs.len() - 1);
    run.planned.len().saturating_sub(usize::from(!edge))
}

/// How well words `run` of `read` read as one change: two points when they
/// end where a part of the code does (on a `;` or a `}`, or before a comma,
/// a line break, a closing bracket or nothing at all), and one when they
/// start where one does (after a `;`, a `}`, a comma, a line break or an
/// opening bracket, or with the code). A `;` or a `}` is the last word of
/// the item it ends, so an item put in among others reads whole.
fn fit(read: &Read, run: Range<usize>) -> u8 {
    let ends_an_item = |word: &str| matches!(word, ";" | "}");
    let closes = run.end == read.words.len()
        || ends_an_item(read.word(run.end - 1))
        || read.gap(run.end - 1).contains([',', '\n'])
        || matches!(read.word(run.end), ")" | "]" | "}");
    let opens = run.start == 0
        || ends_an_item(read.word(run.start - 1))
        || read.gap(run.start - 1).contains([',', '\n'])
        || matches!(read.word(run.start - 1), "(" | "[" | "{");
    u8::from(closes) * 2 + u8::from(opens)
}

/// The change at `at` moved from starting at word `from` of the text that
/// holds its words to starting at `to`: every bound around it moves as far
/// in both codes, the kept runs beside it giving or taking the words it
/// moves past.
fn shift(runs: &mut [Run], at: usize, from: usize, to: usize) {
    let moved = |bound: usize| bound + to - from;
    if let Some(before) = at.checked_sub(1).and_then(|before| runs.get_mut(before)) {
        before.planned.end = moved(before.planned.end);
        before.today.end = moved(before.today.end);
    }
    if let Some(run) = runs.get_mut(at) {
        run.planned = moved(run.planned.start)..moved(run.planned.end);
        run.today = moved(run.today.start)..moved(run.today.end);
    }
    if let Some(after) = runs.get_mut(at + 1) {
        after.planned.start = moved(after.planned.start);
        after.today.start = moved(after.today.start);
    }
}

/// `runs` with each kept run that stands between two changes on one planned
/// line, and says no more than either of them, read as part of one change
/// with both: a word left standing between two replaced ones reads as
/// replaced too.
fn cleaned(mut runs: Vec<Run>, planned: &Read, today: &Read) -> Vec<Run> {
    while let Some(at) =
        (1..runs.len().saturating_sub(1)).find(|&at| absorbed(&runs, at, planned, today))
    {
        let after = runs.remove(at + 1);
        runs.remove(at);
        let before = &mut runs[at - 1];
        before.planned.end = after.planned.end;
        before.today.end = after.today.end;
    }
    runs
}

/// Whether the kept run at `at`, between two changes, reads as part of one
/// change with them: it says no more than either of them, and it stands on
/// one planned line with both.
fn absorbed(runs: &[Run], at: usize, planned: &Read, today: &Read) -> bool {
    let (Some(before), Some(kept), Some(after)) =
        (runs.get(at - 1), runs.get(at), runs.get(at + 1))
    else {
        return false;
    };
    if !kept.kept || before.kept || after.kept {
        return false;
    }
    let size = |read: &Read, words: &Range<usize>| read.spoken(words).chars().count();
    let says = size(planned, &kept.planned);
    let outsaid =
        |change: &Run| says <= size(planned, &change.planned).max(size(today, &change.today));
    let line = planned.place(&before.planned).start..planned.place(&after.planned).end;
    outsaid(before) && outsaid(after) && !planned.text[line].contains('\n')
}

/// `runs` laid out on the planned code's lines, each line as the parts it
/// reads in: what it keeps, what it puts in, and what it takes out where
/// that stood.
fn laid_out(runs: &[Run], planned: &Read, today: &Read) -> Marks {
    let mut added: Vec<Range<usize>> = Vec::new();
    let mut removed: Vec<(usize, String)> = Vec::new();
    for run in runs.iter().filter(|run| !run.kept) {
        added.extend(planned.on_lines(&run.planned));
        if run.today.is_empty() {
            continue;
        }
        removed.push(match run.planned.is_empty() {
            true => (
                planned.after(run.planned.start),
                taken_out(today, &run.today),
            ),
            false => (planned.place(&run.planned).start, today.spoken(&run.today)),
        });
    }
    let mut start = 0;
    let lines = planned
        .text
        .split('\n')
        .map(|said| {
            let line = start..start + said.len();
            start = line.end + 1;
            parts(planned.text, line, &added, &removed)
        })
        .collect();
    Marks { lines }
}

/// The parts the line at `line` of `text` reads in: each stretch of `added`
/// on it, each of `removed` that stands there, and what it keeps between.
fn parts(
    text: &str,
    line: Range<usize>,
    added: &[Range<usize>],
    removed: &[(usize, String)],
) -> Vec<Part> {
    let mut marked: Vec<(Range<usize>, Part)> = removed
        .iter()
        .filter(|(at, _)| (line.start..=line.end).contains(at))
        .map(|(at, said)| (*at..*at, Part::Removed(said.clone())))
        .chain(
            added
                .iter()
                .filter(|stretch| line.contains(&stretch.start))
                .map(|stretch| {
                    (
                        stretch.clone(),
                        Part::Added(text[stretch.clone()].to_string()),
                    )
                }),
        )
        .collect();
    // What comes out stands before what goes in at the same place.
    marked.sort_by_key(|(stretch, _)| (stretch.start, stretch.end));
    let mut parts = Vec::new();
    let mut at = line.start;
    for (stretch, part) in marked {
        if at < stretch.start {
            parts.push(Part::Kept(text[at..stretch.start].to_string()));
        }
        parts.push(part);
        at = at.max(stretch.end);
    }
    if at < line.end {
        parts.push(Part::Kept(text[at..line.end].to_string()));
    }
    parts
}

/// Words `range` of today's code as a change that only takes them out says
/// them: with the comma that stood before them, or else the one after them,
/// so what is left reads as code.
fn taken_out(today: &Read, range: &Range<usize>) -> String {
    let parting = |gap: String| gap.contains(',').then_some(gap);
    let before = range
        .start
        .checked_sub(1)
        .and_then(|at| parting(today.gap(at)));
    let after = (range.end < today.words.len())
        .then(|| today.gap(range.end - 1))
        .and_then(parting);
    let words = today.spoken(range);
    let said = match (before, after) {
        (Some(before), _) => format!("{before}{words}"),
        (None, Some(after)) => format!("{words}{after}"),
        (None, None) => words,
    };
    collapsed(&said).trim_start().to_string()
}

#[cfg(test)]
mod tests;
