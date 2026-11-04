//! Reading the repository's own documents.
//!
//! A document is held to the code by reading it the way a person reads
//! it — the fenced example, the section under a heading, the names in
//! backticks — and comparing what it names against what the types and the
//! binary publish. These readers are the one place that reading is
//! written, so two tests that check two documents never disagree about
//! what a block or a section is.
//!
//! Most readers take text a test already holds; the ones that take a
//! path open it themselves, and a path that cannot be read is a broken
//! fixture, so they panic naming it.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// One fenced block of a document, with where it came from.
pub struct Block {
    /// `path:line` of the first line inside the fence — what a failure
    /// prints so a reader can open the example that broke.
    pub origin: String,
    /// The block's own text, dedented to the fence's column.
    pub text: String,
}

/// Every ```` ```<lang> ```` block of `path`, in the order the document
/// writes them.
///
/// A fence indented under a list item carries that indentation on every
/// line; it is removed here, so a block reads as the document means it
/// rather than as the list that holds it.
pub fn fenced_blocks(path: &Path, lang: &str) -> Vec<Block> {
    let text = read(path);
    let opening = format!("```{lang}");
    let mut blocks = Vec::new();
    let mut open: Option<(usize, usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        match &mut open {
            None if line.trim_start() == opening => open = Some((index + 1, indent, String::new())),
            Some((start, _, body)) if line.trim_start().starts_with("```") => {
                blocks.push(Block {
                    origin: format!("{}:{}", path.display(), start),
                    text: std::mem::take(body),
                });
                open = None;
            }
            Some((_, indent, body)) => {
                body.push_str(line.get(*indent..).unwrap_or(line.trim_start()));
                body.push('\n');
            }
            None => {}
        }
    }
    blocks
}

/// Every `.md` file under `dir`, at any depth, in path order.
pub fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect_markdown(dir, &mut found);
    found.sort();
    found
}

fn collect_markdown(dir: &Path, found: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read `{}`: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            collect_markdown(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            found.push(path);
        }
    }
}

/// The body of the section `heading` opens: every line after it, up to
/// the next heading at its own level or above.
///
/// `heading` carries its own `#` markers (`"## 6.4"`), which is what says
/// the level. A `#` line inside a fenced block is a comment in an
/// example, never a heading.
pub fn section(text: &str, heading: &str) -> String {
    let level = heading.bytes().take_while(|b| *b == b'#').count();
    let mut body = String::new();
    let mut inside = false;
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        let depth = line.bytes().take_while(|b| *b == b'#').count();
        if !fenced && depth > 0 && depth <= level {
            if inside {
                break;
            }
            inside = line.starts_with(heading);
            continue;
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    body
}

/// The spans `text` carries in backticks, in the order it writes them —
/// how a document names a type, a field or a tool.
pub fn backticked(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// Whether `yaml` declares `key` at its top level.
pub fn has_top_level_key(yaml: &str, key: &str) -> bool {
    yaml.lines()
        .any(|line| line.starts_with(key) && line[key.len()..].starts_with(':'))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read `{}`: {e}", path.display()))
}

/// A committed JSON schema, parsed — where a type publishes a closed
/// set of the language it reads.
pub fn json_schema(path: &Path) -> Value {
    serde_json::from_str(&read(path)).expect("a JSON schema")
}
