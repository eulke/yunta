//! `cargo xtask smells [--check]` — the smell ratchet.
//!
//! The audit that this repository's rebuild answered measured a set of
//! patterns (unwrap outside tests, `Result<_, String>`, wall-clock reads,
//! git runners, discarded results, oversized files and functions, copied
//! test helpers). Each is now at or near zero, held there by typed errors,
//! injected clocks, one git module, and a shared test crate. This ratchet
//! keeps them there: it counts every pattern against a committed baseline
//! and, under `--check`, fails naming any count that rose. A number can
//! only ever go down — the day one does, the baseline moves with it in the
//! same change.
//!
//! Counting is line-based (a line matching the pattern), the same measure
//! the rebuild's own acceptance commands used, so a contributor and CI see
//! the same number `grep -c` would.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the committed baseline lives, next to this crate.
fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("smells.baseline")
}

/// The workspace root — the parent of this crate's directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Every `.rs` file under `dir`, recursively; missing directories yield
/// nothing so a counter over a path that does not exist is simply zero.
fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs(dir, &mut out);
    out.sort();
    out
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The `src` directory of every crate under `crates/`.
fn crate_src_dirs() -> Vec<PathBuf> {
    crate_subdirs("src")
}

/// The `tests` directory of every crate under `crates/`.
fn crate_test_dirs() -> Vec<PathBuf> {
    crate_subdirs("tests")
}

fn crate_subdirs(name: &str) -> Vec<PathBuf> {
    let crates = workspace_root().join("crates");
    let mut dirs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&crates) {
        for entry in entries.flatten() {
            let sub = entry.path().join(name);
            if sub.is_dir() {
                dirs.push(sub);
            }
        }
    }
    dirs.sort();
    dirs
}

/// Lines across `files` that satisfy `matches`.
fn count_lines(files: &[PathBuf], matches: impl Fn(&str) -> bool) -> usize {
    files
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.lines().filter(|line| matches(line)).count())
        .sum()
}

/// A source file with its inline `#[cfg(test)]` modules blanked, so a
/// production counter never sees the unit tests that live beside the code:
/// the audit measured these patterns *outside* tests.
fn production_only(text: &str) -> String {
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
fn test_mod_mask(text: &str) -> Vec<bool> {
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

/// Every counter, by name, computed fresh from the tree.
fn measure() -> BTreeMap<String, usize> {
    let src_files: Vec<PathBuf> = crate_src_dirs()
        .iter()
        .flat_map(|dir| rs_files(dir))
        .collect();
    let tests: Vec<PathBuf> = crate_test_dirs()
        .iter()
        .flat_map(|dir| rs_files(dir))
        .collect();
    let cli_src = rs_files(&workspace_root().join("crates/cli/src"));

    // Production text (unit-test modules blanked) for the counters the audit
    // measured outside tests.
    let prod: Vec<String> = src_files
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| production_only(&text))
        .collect();

    let mut counts = BTreeMap::new();

    // Production `unwrap`/`expect`/`panic`/indexing are not counted here:
    // the crate-root `#![deny(clippy::…)]` lints make each a compile error,
    // which CI's `cargo clippy -- -D warnings` already enforces — a stronger
    // gate than a ratchet, and one a comment mention of `.unwrap()` can't
    // inflate.

    // A discarded `Result` hides a degradation the log should carry.
    counts.insert(
        "let_underscore_in_prod".to_string(),
        prod.iter()
            .flat_map(|text| text.lines())
            .filter(|line| line.trim_start().starts_with("let _ ="))
            .count(),
    );
    // A production function near 50 lines is a seam not yet cut.
    counts.insert(
        "prod_fns_over_50_lines".to_string(),
        prod.iter().map(|text| functions_over(text, 50)).sum(),
    );
    // A file near 500 lines is another (counted whole — its tests are part
    // of what a reader scrolls).
    counts.insert(
        "src_files_over_500_lines".to_string(),
        src_files
            .iter()
            .filter(|path| {
                std::fs::read_to_string(path)
                    .map(|text| text.lines().count() > 500)
                    .unwrap_or(false)
            })
            .count(),
    );
    // The wall clock is read in exactly one place — `SystemClock`.
    counts.insert(
        "utc_now_outside_clock".to_string(),
        count_lines(
            &src_files
                .iter()
                .filter(|p| !p.ends_with("clock.rs"))
                .cloned()
                .collect::<Vec<_>>(),
            |line| line.contains("Utc::now"),
        ),
    );
    // Git runs from one module per crate that needs it — the engine's and
    // the test harness's, never scattered. Counted by file, so a third site
    // is what trips it.
    counts.insert(
        "git_command_new_files".to_string(),
        src_files
            .iter()
            .filter(|path| {
                std::fs::read_to_string(path)
                    .map(|text| text.contains("Command::new(\"git\")"))
                    .unwrap_or(false)
            })
            .count(),
    );
    // The CLI has one error translator; `main` maps the exit code.
    counts.insert(
        "exit_failure_in_cli".to_string(),
        count_lines(&cli_src, |line| line.contains("ExitCode::FAILURE")),
    );
    // Test infrastructure lives in the support crate, never copied into a
    // test file.
    counts.insert(
        "copied_test_helpers".to_string(),
        count_lines(&tests, |line| {
            let t = line.trim_start();
            t.starts_with("fn git(")
                || t.starts_with("fn yunta_in(")
                || t.starts_with("fn init_repo(")
                || t.starts_with("struct FixedClock")
        }),
    );

    counts
}

/// How many function bodies in `source` exceed `max` lines, measured from
/// the line after the body's opening `{` to its matching `}` — the span a
/// reader scrolls. String and char literals and comments are blanked first
/// so a brace inside them never opens or closes a body. A deliberate
/// heuristic, not a parser: it only ever gates *growth* against a baseline,
/// so an occasional miscount is stable and harmless.
fn functions_over(source: &str, max: usize) -> usize {
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
fn strip_noise(line: &str, mut in_block: bool) -> (String, bool) {
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

fn render(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("{name} {count}\n"))
        .collect()
}

fn parse_baseline(text: &str) -> BTreeMap<String, usize> {
    text.lines()
        .filter_map(|line| {
            let (name, count) = line.rsplit_once(' ')?;
            Some((name.to_string(), count.trim().parse().ok()?))
        })
        .collect()
}

/// `cargo xtask smells`: measure and write the baseline.
pub fn write() -> Result<(), String> {
    let counts = measure();
    let path = baseline_path();
    std::fs::write(&path, render(&counts))
        .map_err(|e| format!("cannot write `{}`: {e}", path.display()))?;
    print!("{}", render(&counts));
    println!("wrote {}", path.display());
    Ok(())
}

/// `cargo xtask smells --check`: fail if any count rose above the baseline,
/// or if the baseline is missing a counter the tree now measures.
pub fn check() -> Result<(), String> {
    let counts = measure();
    let path = baseline_path();
    let baseline = parse_baseline(&std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "cannot read `{}` — run `cargo xtask smells`: {e}",
            path.display()
        )
    })?);

    let mut risen = Vec::new();
    for (name, &count) in &counts {
        match baseline.get(name) {
            Some(&allowed) if count <= allowed => {}
            Some(&allowed) => risen.push(format!("  {name}: {allowed} -> {count}")),
            None => risen.push(format!("  {name}: (new) -> {count}")),
        }
    }
    if risen.is_empty() {
        println!("smells: no count rose above the baseline");
        return Ok(());
    }
    Err(format!(
        "these smell counts rose above `{}` — fix them, or lower the baseline in the same \
         change if the rise is deliberate:\n{}",
        path.display(),
        risen.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
