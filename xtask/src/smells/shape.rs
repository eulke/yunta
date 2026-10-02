//! Reading the shape of a source file, for the counters that measure it.
//!
//! A ratchet that counts by line has to know which lines are which: the
//! ones inside a `#[cfg(test)]` module, the ones inside a string or a
//! comment where a brace means nothing, and the span from a function's
//! opening brace to its matching close. These are deliberate heuristics
//! rather than a parser: each only ever gates *growth* against a
//! baseline, so a stable miscount costs nothing and a wrong answer about
//! one line never changes a verdict about the tree.

/// A source file with its inline `#[cfg(test)]` modules blanked, so a
/// production counter never sees the unit tests that live beside the code:
/// the audit measured these patterns *outside* tests.
pub fn production_only(text: &str) -> String {
    let mask = test_mod_mask(text);
    text.lines()
        .zip(mask)
        .map(|(line, in_test)| {
            if in_test {
                String::new()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One flag per line: whether it sits inside a `#[cfg(test)]` module (the
/// attribute line, the `mod … {` header, and the body through its matching
/// `}`). Braces inside strings and comments are ignored via [`strip_noise`],
/// so a `{` in a string never opens a phantom module.
pub fn test_mod_mask(text: &str) -> Vec<bool> {
    let lines: Vec<&str> = text.lines().collect();
    let blanked = blanked_lines(text);
    let mut mask = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if blanked[i].contains("#[cfg(test)]") {
            // Find the block this attribute guards (a `mod`) and its opening
            // brace; a `#[cfg(test)]` on anything else is left alone.
            let mut j = i;
            let mut is_mod = false;
            while j < lines.len() {
                if blanked[j].contains(" mod ") || blanked[j].trim_start().starts_with("mod ") {
                    is_mod = true;
                }
                if blanked[j].contains('{') {
                    break;
                }
                if blanked[j].contains(';') {
                    break;
                }
                j += 1;
            }
            if is_mod && j < lines.len() && blanked[j].contains('{') {
                // A module whose braces never balance — a `/*` inside a
                // multi-line string opens a comment that never closes —
                // runs to the end of the file.
                let mut depth = 0i32;
                let mut close = lines.len() - 1;
                for (k, line) in blanked.iter().enumerate().skip(j) {
                    depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                    if depth == 0 {
                        close = k;
                        break;
                    }
                }
                for flag in mask.iter_mut().take(close + 1).skip(i) {
                    *flag = true;
                }
                i = close + 1;
                continue;
            }
        }
        i += 1;
    }
    mask
}

/// Whether `line` carries a string literal with eight or more spaces
/// between two visible characters — the trace a `\` line continuation
/// leaves when it goes missing: the message reaches its reader with a
/// source line's indentation inside it, never shallower than two
/// nesting levels. Alignment an author writes into a message (a table
/// column, a commented YAML template) uses a few spaces, and spaces a
/// literal opens with or that follow a `\n` escape are layout; comment
/// lines are not messages.
pub fn has_inner_space_run(line: &str) -> bool {
    if line.trim_start().starts_with("//") {
        return false;
    }
    let Some(open) = line.find('"') else {
        return false;
    };
    let bytes = line.as_bytes();
    let mut i = open + 1;
    while let Some(&byte) = bytes.get(i) {
        if byte != b' ' {
            i += 1;
            continue;
        }
        let start = i;
        while bytes.get(i) == Some(&b' ') {
            i += 1;
        }
        let before = start.checked_sub(1).and_then(|j| bytes.get(j));
        let after_escape = start >= 2 && bytes.get(start - 2..start) == Some(b"\\n".as_slice());
        let visible_before = before.is_some_and(|&b| b != b'"' && b != b'\\');
        let visible_after = bytes.get(i).is_some_and(|&b| b != b'"');
        if i - start >= 8 && visible_before && !after_escape && visible_after {
            return true;
        }
    }
    false
}

/// Whether `line` hedges a count with a plural in parentheses —
/// `3 file(s)`, `2 child(ren)`, `a process(es)` — where the number beside
/// it already says how many: a word written just before a parenthesis
/// holding a plural's ending, after a space or at the start of a
/// continued line, which is prose and never a call (`Some(s)`,
/// `str::trim(s)`). Comment lines are not messages.
pub fn has_parenthesized_plural(line: &str) -> bool {
    if line.trim_start().starts_with("//") {
        return false;
    }
    line.match_indices('(').any(|(at, _)| {
        let after = &line[at + 1..];
        let plural = ["s)", "es)", "ies)", "ren)"]
            .iter()
            .any(|ending| after.starts_with(ending));
        if !plural {
            return false;
        }
        let before = &line[..at];
        let word = before
            .trim_end_matches(|c: char| c.is_ascii_lowercase() || c == '-')
            .len();
        word < at
            && before[..word]
                .chars()
                .last()
                .is_none_or(char::is_whitespace)
    })
}

/// How many function bodies in `source` exceed `max` lines, measured from
/// the line after the body's opening `{` to its matching `}` — the span a
/// reader scrolls. String and char literals and comments are blanked first
/// so a brace inside them never opens or closes a body. A deliberate
/// heuristic, not a parser: it only ever gates *growth* against a baseline,
/// so an occasional miscount is stable and harmless.
pub fn functions_over(source: &str, max: usize) -> usize {
    bodies(&blanked_lines(source))
        .iter()
        .filter(|body| body.lines() > max)
        .count()
}

/// Lines calling `std::fs::` inside the body of an `async fn`.
///
/// A synchronous file call blocks the thread the runtime gave the task:
/// every other task sharing that thread waits for the disk, and nothing
/// in the code says so. Disk work inside an async body goes
/// through the async API or through `spawn_blocking`.
pub fn sync_fs_in_async(source: &str) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    let blanked = blanked_lines(source);
    bodies(&blanked)
        .iter()
        .filter(|body| blanked[body.header].contains("async fn"))
        .map(|body| {
            lines
                .iter()
                .skip(body.open + 1)
                .take(body.lines())
                .filter(|line| line.contains("std::fs::"))
                .count()
        })
        .sum()
}

/// The span of a function body: the line its `fn` header opens, the line
/// its body's `{` opens, and the line its matching `}` closes.
struct Body {
    header: usize,
    open: usize,
    close: usize,
}

impl Body {
    /// How many lines a reader scrolls between the braces.
    fn lines(&self) -> usize {
        self.close.saturating_sub(self.open + 1)
    }
}

/// Every function body in `blanked`, in source order. A function nested
/// inside another is part of the body that holds it, so each span is
/// reported once, by its outermost function.
fn bodies(blanked: &[String]) -> Vec<Body> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < blanked.len() {
        // A function header is a line whose code contains `fn <name>(`.
        if find_fn(&blanked[i]).is_some() {
            // Walk to the body's opening brace (it may be on a later line
            // for a multi-line signature or `where` clause).
            let mut j = i;
            let mut opened = false;
            while j < blanked.len() {
                if blanked[j].contains('{') {
                    opened = true;
                    break;
                }
                if blanked[j].contains(';') {
                    break; // a `fn` declaration with no body (trait method).
                }
                j += 1;
            }
            if opened {
                let mut depth = 0i32;
                let mut close = j;
                'body: for (k, line) in blanked.iter().enumerate().skip(j) {
                    for ch in line.chars() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                close = k;
                                break 'body;
                            }
                        }
                    }
                }
                out.push(Body {
                    header: i,
                    open: j,
                    close,
                });
                i = close + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Every line of `source` with its string literals and comments blanked,
/// so only structural braces survive.
fn blanked_lines(source: &str) -> Vec<String> {
    let mut in_block = false;
    source
        .lines()
        .map(|line| {
            let (out, next) = strip_noise(line, in_block);
            in_block = next;
            out
        })
        .collect()
}

/// The column where a `fn` keyword introduces a function, or `None`. Only a
/// `fn` at a word boundary counts, so `transfn` or a `fn` inside an
/// already-blanked string never matches.
fn find_fn(code: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut idx = 0;
    while let Some(pos) = code[idx..].find("fn ") {
        let at = idx + pos;
        let before_ok = at == 0 || !is_ident(bytes[at - 1]);
        if before_ok {
            return Some(at);
        }
        idx = at + 2;
    }
    None
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Blanks string/char literals and comments in `line` (replacing them with
/// spaces) so only structural braces survive; returns the blanked line and
/// whether a block comment is still open at its end.
pub fn strip_noise(line: &str, mut in_block: bool) -> (String, bool) {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_block {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                in_block = false;
                out.push_str("  ");
                i += 2;
                continue;
            }
            out.push(' ');
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            break; // line comment — the rest is noise.
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            in_block = true;
            out.push_str("  ");
            i += 2;
            continue;
        }
        if c == '"' {
            out.push(' ');
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    out.push_str("  ");
                    i += 2;
                    continue;
                }
                if chars[i] == '"' {
                    out.push(' ');
                    i += 1;
                    break;
                }
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '\'' {
            // A char literal is `'x'` or `'\n'`/`'\u{..}'`; anything else
            // opening with `'` is a lifetime (`'static`, `'a`), emitted
            // verbatim so the code and braces after it are still seen.
            let is_char = chars.get(i + 1) == Some(&'\\') || chars.get(i + 2) == Some(&'\'');
            if !is_char {
                out.push(c);
                i += 1;
                continue;
            }
            out.push(' ');
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    out.push_str("  ");
                    i += 2;
                    continue;
                }
                if chars[i] == '\'' {
                    out.push(' ');
                    i += 1;
                    break;
                }
                out.push(' ');
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    (out, in_block)
}

/// Whether `line` formats a value with its `Debug` form in text that
/// reaches a reader — a person or an agent. `Debug` is the shape Rust
/// gives a value for whoever wrote it: `Some(..)`, `TaskRecord { .. }`,
/// `Major`. An assertion's, a panic's or a trace's message is for the
/// one who wrote the code, and is not counted.
pub fn has_debug_format(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") || !(code.contains(":?}") || code.contains(":#?}")) {
        return false;
    }
    ![
        "assert",
        "panic!",
        "unreachable!",
        "debug!",
        "trace!",
        "tracing::",
        "todo!",
    ]
    .iter()
    .any(|developer| code.contains(developer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_space_runs_are_the_trace_of_a_lost_continuation() {
        assert!(has_inner_space_run(
            r#"    "the adapter declares no                 session resume""#
        ));
        // A few spaces align a column; the trace is a source line's indentation.
        assert!(!has_inner_space_run(
            r#"    "  {} {:>3} run(s)    median CPTV {}    median tokens {}","#
        ));
        // Indentation a literal opens with is layout, not a trace.
        assert!(!has_inner_space_run(
            r#"    println!("      tradeoff: {}", x);"#
        ));
        // So is indentation after a `\n` escape, and a comment is not a message.
        assert!(!has_inner_space_run(
            r#"    "add e.g.:\n      runners:\n        \"#
        ));
        assert!(!has_inner_space_run("    // a          comment"));
        assert!(!has_inner_space_run("    let x = 1;"));
    }

    #[test]
    fn a_debug_form_in_a_message_counts_and_one_for_a_developer_does_not() {
        assert!(has_debug_format(
            r#"        .map(|(id, status)| format!("{id}: {status:?}"))"#
        ));
        assert!(has_debug_format(
            r#"    #[error("failed to read {stream:?} for `{command}`")]"#
        ));
        assert!(has_debug_format(
            r#"                "{} at or above {max_severity:?}: {}","#
        ));
        assert!(!has_debug_format(
            r#"        assert_eq!(a, b, "{drawn:?}");"#
        ));
        assert!(!has_debug_format(
            r#"        other => panic!("expected it, got {other:?}"),"#
        ));
        assert!(!has_debug_format(
            r#"    tracing::debug!("state {state:?}");"#
        ));
        assert!(!has_debug_format("    // prints {value:?} for now"));
        assert!(!has_debug_format(r#"    format!("{value}")"#));
    }

    #[test]
    fn a_parenthesized_plural_in_a_string_counts_and_one_in_a_comment_does_not() {
        assert!(has_parenthesized_plural(
            r#"    "scope violated: {} file(s) outside the declared globs","#
        ));
        // A continued line of a message opens on the word.
        assert!(has_parenthesized_plural(
            r#"             artifact(s) it names: {}","#
        ));
        assert!(has_parenthesized_plural(r#"    "{} re-route(s)","#));
        assert!(has_parenthesized_plural(r#"    "{} child(ren) finished","#));
        assert!(!has_parenthesized_plural(r#"    "call (with care)","#));
        assert!(!has_parenthesized_plural("    // every file(s) it names"));
        assert!(!has_parenthesized_plural(
            "    Value::String(s) => s.clone(),"
        ));
        assert!(!has_parenthesized_plural("    let t = str::trim(s);"));
        assert!(!has_parenthesized_plural("    let t = Some(s);"));
    }

    #[test]
    fn functions_over_counts_only_bodies_past_the_budget() {
        let short = "fn a() {\n    let x = 1;\n}\n";
        assert_eq!(functions_over(short, 5), 0);
        let long = format!("fn b() {{\n{}}}\n", "    let x = 1;\n".repeat(10));
        assert_eq!(functions_over(&long, 5), 1);
    }

    #[test]
    fn functions_over_ignores_a_brace_inside_a_string() {
        // The `{` in the string must not open a body, and the trait method
        // declaration with no body must not count.
        let source =
            "fn a() -> &'static str {\n    \"} not a brace {\"\n}\ntrait T { fn m(&self); }\n";
        assert_eq!(functions_over(source, 0), 1);
    }

    #[test]
    fn sync_fs_counts_only_the_calls_inside_an_async_body() {
        let source = "\
async fn reads() {
    let text = std::fs::read_to_string(path);
}
fn also_reads() {
    let text = std::fs::read_to_string(path);
}
";
        assert_eq!(sync_fs_in_async(source), 1);
    }

    #[test]
    fn a_test_module_whose_braces_never_balance_runs_to_the_end_of_the_file() {
        let source = "fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        docs/**\n}\n";
        assert_eq!(
            test_mod_mask(source),
            vec![false, true, true, true, true, true]
        );
    }

    #[test]
    fn test_mod_mask_covers_a_cfg_test_module() {
        let source =
            "fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn also_prod() {}\n";
        let mask = test_mod_mask(source);
        assert_eq!(
            mask,
            vec![false, true, true, true, true, false],
            "the attribute, header and body are masked; production lines are not"
        );
        // Production-only text keeps prod, drops the test module's contents.
        let prod = production_only(source);
        assert!(prod.contains("fn prod()") && prod.contains("fn also_prod()"));
        assert!(!prod.contains("fn t()"));
    }
}
