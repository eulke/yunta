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
    let blanked = {
        let mut in_block = false;
        lines
            .iter()
            .map(|line| {
                let (out, next) = strip_noise(line, in_block);
                in_block = next;
                out
            })
            .collect::<Vec<_>>()
    };
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
                let mut depth = 0i32;
                for (k, line) in blanked.iter().enumerate().skip(j) {
                    for ch in line.chars() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                        }
                    }
                    for flag in mask.iter_mut().take(k + 1).skip(i) {
                        *flag = true;
                    }
                    if depth == 0 {
                        i = k + 1;
                        break;
                    }
                }
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

/// How many function bodies in `source` exceed `max` lines, measured from
/// the line after the body's opening `{` to its matching `}` — the span a
/// reader scrolls. String and char literals and comments are blanked first
/// so a brace inside them never opens or closes a body. A deliberate
/// heuristic, not a parser: it only ever gates *growth* against a baseline,
/// so an occasional miscount is stable and harmless.
pub fn functions_over(source: &str, max: usize) -> usize {
    let blanked: Vec<String> = {
        let mut in_block = false;
        source
            .lines()
            .map(|line| {
                let (out, next) = strip_noise(line, in_block);
                in_block = next;
                out
            })
            .collect()
    };
    let mut over = 0;
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
                let open_line = j;
                let mut close_line = j;
                'body: for (k, line) in blanked.iter().enumerate().skip(j) {
                    for ch in line.chars() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                close_line = k;
                                break 'body;
                            }
                        }
                    }
                }
                let body_lines = close_line.saturating_sub(open_line + 1);
                if body_lines > max {
                    over += 1;
                }
                i = close_line + 1;
                continue;
            }
        }
        i += 1;
    }
    over
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
