//! Laying text out for a surface that has a shape to respect.
//!
//! A terminal row has one line, a Markdown bullet has an indent, and a
//! list of problems has a heading that counts them. None of that is a
//! property of what is being said, so it lives here rather than in the
//! types that say it — and once, rather than in each surface, because a
//! renderer that forgets the collapse does not produce a worse line, it
//! produces a broken table or an unparseable graph.

use std::fmt;

/// Whitespace collapsed to one line, for a place with room for exactly
/// one: a graph label, a row in a listing, a summary.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every line after the first prefixed, so a block hangs under the line
/// that introduced it and each line keeps the one it was written on.
pub fn hanging(text: &str, prefix: &str) -> String {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let mut out = first.to_string();
    for line in lines {
        out.push('\n');
        out.push_str(prefix);
        out.push_str(line);
    }
    out
}

/// A headline and the detail that explains it, joined by `": "`, and the
/// headline alone when the detail says nothing.
///
/// A colon promises a reader that something follows it, so nothing
/// promises it when a source of detail — a subprocess that wrote no
/// stderr, a failure whose cause carries no message — came back empty.
/// Surrounding whitespace is not detail either: `"  \n"` reads as
/// nothing said.
///
/// ```
/// # use surco::text::detailed;
/// assert_eq!(detailed("exit 1", "no such file"), "exit 1: no such file");
/// assert_eq!(detailed("exit 1", ""), "exit 1");
/// ```
pub fn detailed(headline: impl fmt::Display, detail: &str) -> String {
    let detail = detail.trim();
    if detail.is_empty() {
        return headline.to_string();
    }
    format!("{headline}: {detail}")
}

/// A subject and what it carries, in parentheses after it, and the subject
/// alone when there is nothing to set off.
///
/// The parentheses promise a reader what [`detailed`]'s colon does; what
/// chooses between them is the shape of what follows. A colon introduces a
/// value, so it reads wrong in front of something that carries colons of
/// its own: a claim followed by the labelled facts behind it, or an event
/// line followed by the free text a payload holds. The parentheses set
/// those aside instead of introducing them.
///
/// ```
/// # use surco::text::aside;
/// assert_eq!(aside("run_paused", "cap: 400; spent: 500"), "run_paused (cap: 400; spent: 500)");
/// assert_eq!(aside("run_paused", ""), "run_paused");
/// ```
pub fn aside(subject: impl fmt::Display, carried: &str) -> String {
    let carried = carried.trim();
    if carried.is_empty() {
        return subject.to_string();
    }
    format!("{subject} ({carried})")
}

/// `n` things, named: `1 case`, `2 cases`. The one place a count already
/// in hand becomes a phrase, so no message hedges with `(s)` while the
/// number sits right beside it.
///
/// `noun` takes a plain `-s` plural; [`counted_as`] takes one that
/// does not.
///
/// The phrase is one string, so its width varies with the count. A
/// column that right-aligns its number (`{:>3}`) has to keep the two
/// apart — format the count itself and follow it with the noun — or the
/// column goes ragged the first time a total reaches two digits.
pub fn counted(n: usize, noun: &str) -> String {
    counted_as(n, noun, &format!("{noun}s"))
}

/// `n` as a place in an order: `1st`, `2nd`, `3rd`, `11th`, `16th`,
/// `21st`.
pub fn ordinal(n: usize) -> String {
    let suffix = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// `n` things, named by a noun whose plural is not a plain `-s`:
/// `1 capability the adapter lacks`, `2 capabilities the adapter lacks`.
pub fn counted_as(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", agreeing(n, one, many))
}

/// The word that agrees with a count of `n`: `one` for exactly one,
/// `many` otherwise — for a sentence whose count is a list rather than a
/// number (`unknown keys `a`, `b``), or a verb that follows one.
pub fn agreeing<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    match n {
        1 => one,
        _ => many,
    }
}

/// The candidate `typed` most likely misspells: within two edits of it,
/// and closer than every other. `None` when nothing is that close, when
/// two are equally close, or when the edits would be most of what was
/// typed — a suggestion is offered only when it is the one a reader
/// meant.
pub fn nearest<'a>(typed: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    /// The edits a misspelling is taken to be within: a slip of one key,
    /// or two letters swapped.
    const NEAR: usize = 2;
    let room = NEAR.min(typed.chars().count().saturating_sub(1));
    let mut best: Option<(&str, usize)> = None;
    let mut tied = false;
    for candidate in candidates {
        let edits = strsim::levenshtein(typed, candidate);
        if candidate == typed || edits > room {
            continue;
        }
        match best {
            Some((held, closest)) if edits == closest && held != candidate => tied = true,
            Some((_, closest)) if edits >= closest => {}
            _ => {
                best = Some((candidate, edits));
                tied = false;
            }
        }
    }
    best.filter(|_| !tied).map(|(candidate, _)| candidate)
}

/// `did you mean `name`?`: the question every refusal that suggests a
/// name asks, of the name `near` that what it refused was a slip away from.
pub fn suggested(near: &str) -> String {
    format!("did you mean `{near}`?")
}

/// `; did you mean `name`?`, said after a name nothing answers to when
/// `near` is the name it was a slip away from; nothing otherwise.
pub fn near_miss(near: Option<&str>) -> String {
    hinted(near.map(suggested))
}

/// `hints` as a line of text says them after its sentence, each after a
/// semicolon; nothing when there are none.
pub fn hinted(hints: impl IntoIterator<Item = String>) -> String {
    hints.into_iter().map(|hint| format!("; {hint}")).collect()
}

/// The [`near_miss`] of `typed` among `candidates`: the one [`nearest`] to it.
pub fn did_you_mean<'a>(typed: &str, candidates: impl IntoIterator<Item = &'a str>) -> String {
    near_miss(nearest(typed, candidates))
}

/// Identifiers as a reader sees a list of them, each in its own
/// backticks — the one joiner every sentence about a set of ids uses.
pub fn listed<'a>(ids: impl IntoIterator<Item = &'a str>) -> String {
    ids.into_iter()
        .map(|id| format!("`{id}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The problems block: a heading naming what was read
/// and how many problems it has, then one indented line per problem.
///
/// ```text
/// workflows/ship.yaml: 2 errors
///   the document: unknown key `version`; the only top-level key is `nodes`
///   node `plan`: `runner` is empty
/// ```
///
/// Every surface that lists what is wrong with one file goes through
/// here, so a reader meets the same block whether a workflow failed to
/// check or an artifact failed to close.
pub fn problems(heading: impl fmt::Display, items: &[impl fmt::Display]) -> String {
    let mut text = format!("{heading}: {}", counted(items.len(), "error"));
    for item in items {
        text.push_str(&format!("\n  {item}"));
    }
    text
}

/// Escapes a Mermaid node label. Labels sit inside `["..."]` and Mermaid
/// renders them as HTML, so every character HTML or the quoting reads
/// specially becomes an entity. `&` is handled in the same single pass as
/// the rest, so an entity this inserts is never re-escaped.
///
/// A label is one line by construction: the collapse is [`one_line`]'s,
/// the rule every surface with room for one line reads.
pub fn escape_mermaid(text: &str) -> String {
    let text = one_line(text);
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{aside, counted, detailed, did_you_mean, nearest};

    #[test]
    fn a_place_in_an_order_takes_the_suffix_it_is_read_with() {
        let read: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 16, 21, 22, 23, 101, 111]
            .into_iter()
            .map(super::ordinal)
            .collect();
        assert_eq!(
            read,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "16th", "21st", "22nd", "23rd",
                "101st", "111th"
            ]
        );
    }

    #[test]
    fn an_unknown_key_suggests_the_key_one_typo_away() {
        let keys = ["depends_on", "scope", "prompt", "runner"];
        assert_eq!(nearest("promt", keys), Some("prompt"));
        assert_eq!(nearest("depend_on", keys), Some("depends_on"));
        assert_eq!(did_you_mean("scpe", keys), "; did you mean `scope`?");
    }

    #[test]
    fn a_name_far_from_every_candidate_or_between_two_suggests_nothing() {
        assert_eq!(nearest("artifacts", ["scope", "runner"]), None, "too far");
        assert_eq!(nearest("cat", ["bat", "hat"]), None, "two are as close");
        assert_eq!(
            nearest("ab", ["xy"]),
            None,
            "the edits would be the whole word"
        );
        assert_eq!(
            nearest("scope", ["scope"]),
            None,
            "a name is not its own near miss"
        );
        assert_eq!(did_you_mean("artifacts", ["scope"]), "");
    }

    #[test]
    fn a_count_names_its_noun_in_the_number_it_is() {
        assert_eq!(counted(0, "run"), "0 runs");
        assert_eq!(counted(1, "run"), "1 run");
        assert_eq!(counted(2, "run"), "2 runs");
        assert_eq!(counted(1, "task instance"), "1 task instance");
    }

    #[test]
    fn a_headline_with_detail_is_joined_by_a_colon() {
        assert_eq!(
            detailed("exit 1", "cannot open `x`"),
            "exit 1: cannot open `x`"
        );
    }

    #[test]
    fn a_headline_whose_detail_is_empty_keeps_no_colon_promising_one() {
        assert_eq!(detailed("exit 1", ""), "exit 1");
    }

    #[test]
    fn whitespace_is_not_detail() {
        assert_eq!(detailed("exit 1", "  \n\t "), "exit 1");
    }

    #[test]
    fn detail_keeps_its_own_lines_and_loses_only_its_margins() {
        assert_eq!(
            detailed("exit 2", "\nfirst\nsecond\n"),
            "exit 2: first\nsecond"
        );
    }

    #[test]
    fn a_subject_with_something_to_carry_carries_it_in_parentheses() {
        assert_eq!(
            aside("run_paused", "cap: 400; spent: 500"),
            "run_paused (cap: 400; spent: 500)"
        );
    }

    #[test]
    fn a_subject_carrying_nothing_keeps_no_parentheses_promising_it() {
        assert_eq!(aside("run_paused", ""), "run_paused");
        assert_eq!(aside("run_paused", "  \n\t "), "run_paused");
    }
}
