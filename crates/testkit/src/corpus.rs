//! Reading the repository's own documents.
//!
//! A document is held to the code by reading it the way a person reads
//! it — the fenced example, the table under a heading, the names in
//! backticks — and comparing what it names against what the types
//! publish. These readers are the one place that reading is written, so
//! two tests that check two documents against two closed sets never
//! disagree about what a table row or a list item is.
//!
//! Most readers take text a test already holds; the ones that take a
//! path open it themselves, and a path that cannot be read is a broken
//! fixture, so they panic naming it.

use std::collections::{BTreeMap, BTreeSet};
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

/// The rows of every markdown table in `body`, each cell trimmed and its
/// `\|` escapes resolved. Header and separator rows are not rows: what a
/// table states is what its body states.
pub fn table_rows(body: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in body.lines().map(str::trim) {
        if !line.starts_with('|') {
            continue;
        }
        let cells = cells_of(line);
        if cells
            .iter()
            .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'))
        {
            // The separator: what precedes it is the header.
            rows.pop();
            continue;
        }
        rows.push(cells);
    }
    rows
}

/// One row's cells. A `\|` belongs to the cell it sits in — it is how a
/// table writes an alternation — so the split is on bare pipes only.
fn cells_of(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut escaped = false;
    for c in line.trim_matches('|').chars() {
        match c {
            '\\' if !escaped => escaped = true,
            '|' if !escaped => cells.push(String::new()),
            _ => {
                let cell = cells.last_mut().expect("a row is open");
                if escaped && c != '|' {
                    cell.push('\\');
                }
                cell.push(c);
                escaped = false;
            }
        }
    }
    cells.iter().map(|cell| cell.trim().to_string()).collect()
}

/// The `- ` items of `body`, each carrying the indented lines that
/// continue it.
pub fn bullets(body: &str) -> Vec<String> {
    items(body, |line| line.starts_with("- "))
}

/// The numbered items of `body`, each carrying the indented lines that
/// continue it.
pub fn numbered_items(body: &str) -> Vec<String> {
    items(body, is_numbered)
}

fn is_numbered(line: &str) -> bool {
    line.split_once(". ")
        .is_some_and(|(number, _)| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

/// The items `opens` recognizes, each joined with the indented lines that
/// follow it up to the blank line that ends it.
fn items(body: &str, opens: impl Fn(&str) -> bool) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut open = false;
    for line in body.lines() {
        if opens(line) {
            items.push(line.to_string());
            open = true;
        } else if open && line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
            let item = items.last_mut().expect("a continuation follows an item");
            item.push(' ');
            item.push_str(line.trim());
        } else {
            open = false;
        }
    }
    items
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

/// The sentence the line opening with `lead` states: from that label to
/// the period that ends it. `None` when no line opens with `lead`.
pub fn sentence_after(body: &str, lead: &str) -> Option<String> {
    let rest = body.lines().find_map(|line| line.strip_prefix(lead))?;
    let end = rest.find(". ").map_or(rest.len(), |at| at + 1);
    Some(rest[..end].trim().to_string())
}

/// The number `text` states immediately before `word` — the count a
/// sentence claims for the thing it names.
pub fn number_before(text: &str, word: &str) -> Option<usize> {
    let at = text.find(word)?;
    let before = text[..at].trim_end();
    let digits = before.trim_end_matches(|c: char| c.is_ascii_digit());
    before[digits.len()..].parse().ok()
}

/// The `pub` field names of `struct name` as a Rust fence declares it —
/// the shape a document publishes, read from the one struct asked for
/// rather than from everything the fence happens to carry.
pub fn struct_fields(rust: &str, name: &str) -> Vec<String> {
    let opening = format!("struct {name} {{");
    rust.lines()
        .skip_while(|line| {
            !line
                .trim_start()
                .trim_start_matches("pub ")
                .starts_with(&opening)
        })
        .skip(1)
        .take_while(|line| *line != "}")
        .filter_map(|line| line.trim().strip_prefix("pub "))
        .filter_map(|field| field.split_once(':'))
        .map(|(name, _)| name.to_string())
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

/// Every variant of an internally tagged enum in a committed JSON
/// schema: the tag each one carries, the fields of the payload it
/// refers to, and whether each field is an `Option`.
///
/// A field that admits `null` is one the value may leave out;
/// everything else it always carries, a `default` saying only what an
/// older document reads as. A payload that is a choice of shapes
/// carries the fields of every branch, flat, which is how it is
/// written.
pub fn tagged_variants(schema: &Path, union: &str) -> BTreeMap<String, BTreeMap<String, bool>> {
    let parsed = json_schema(schema);
    let mut variants = BTreeMap::new();
    for variant in parsed["$defs"][union]["oneOf"]
        .as_array()
        .unwrap_or_else(|| panic!("`{union}` is a tagged union in `{}`", schema.display()))
    {
        let tag = variant["properties"]["kind"]["const"]
            .as_str()
            .expect("a variant carries its tag");
        let named = variant["$ref"]
            .as_str()
            .and_then(|reference| reference.rsplit('/').next())
            .expect("a variant refers to its payload");
        let payload = &parsed["$defs"][named];
        let mut fields = BTreeMap::new();
        let mut take = |object: &Value| {
            for (field, shape) in object["properties"].as_object().into_iter().flatten() {
                fields.insert(field.clone(), admits_null(shape));
            }
        };
        take(payload);
        for branch in payload["anyOf"].as_array().into_iter().flatten() {
            take(branch);
        }
        variants.insert(tag.to_string(), fields);
    }
    variants
}

fn admits_null(shape: &Value) -> bool {
    shape["type"]
        .as_array()
        .is_some_and(|types| types.iter().any(|kind| kind == "null"))
        || shape["anyOf"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|branch| branch["type"] == "null")
}

/// What a spec's field tables state each subject carries: the fields,
/// and whether each is one the subject may leave out.
///
/// Four reading rules, which are how such a spec is written. A heading
/// starting with `opens` names in backticks the subjects its tables
/// cover. A line that names one of them and ends in `:` opens the table
/// belonging to that subject alone, and an obligation cell that names
/// one restricts its row the same way. A row is `campo | tipo |
/// obligatorio | …`, and states a field of an object by its path
/// (`finding.id`), the field being the root of it. A row is optional
/// where its type says `Option<…>` or its obligation says `no`; where
/// two rows state one field, mandatory wins.
pub fn field_tables(text: &str, opens: &str) -> BTreeMap<String, BTreeMap<String, bool>> {
    let mut stated: BTreeMap<String, BTreeMap<String, bool>> = BTreeMap::new();
    for heading in text.lines().filter(|line| line.starts_with(opens)) {
        let subjects = backticked(heading);
        for (active, chunk) in labelled_chunks(&section(text, heading), &subjects) {
            for row in table_rows(&chunk).iter().filter(|row| row.len() >= 3) {
                let named: Vec<String> = backticked(&row[2])
                    .into_iter()
                    .filter(|name| subjects.contains(name))
                    .collect();
                let optional = row[2].starts_with("no")
                    || backticked(&row[1])
                        .iter()
                        .any(|ty| ty.starts_with("Option<"));
                for subject in if named.is_empty() { &active } else { &named } {
                    state(&mut stated, subject, &row[0], optional);
                }
            }
        }
    }
    stated
}

/// `body` cut where a line names one of `subjects` and ends in `:`,
/// each piece with the subjects its tables speak for.
fn labelled_chunks(body: &str, subjects: &[String]) -> Vec<(Vec<String>, String)> {
    let mut chunks = vec![(subjects.to_vec(), String::new())];
    for line in body.lines() {
        match line.strip_suffix(':').map(backticked) {
            Some(named) if named.len() == 1 && subjects.contains(&named[0]) => {
                chunks.push((named, String::new()));
            }
            _ => {
                let open = chunks.last_mut().expect("a chunk is open");
                open.1.push_str(line);
                open.1.push('\n');
            }
        }
    }
    chunks
}

fn state(
    stated: &mut BTreeMap<String, BTreeMap<String, bool>>,
    subject: &str,
    cell: &str,
    optional: bool,
) {
    for name in backticked(cell) {
        let root = name.split('.').next().unwrap_or(&name).to_string();
        stated
            .entry(subject.to_string())
            .or_default()
            .entry(root)
            .and_modify(|known| *known &= optional)
            .or_insert(optional);
    }
}

/// A committed JSON schema, parsed — where a type publishes a closed
/// set of the language it reads.
pub fn json_schema(path: &Path) -> Value {
    serde_json::from_str(&read(path)).expect("a JSON schema")
}

/// The `const` each of `alternatives` fixes for `key`: the closed set a
/// schema publishes as a choice of shapes.
pub fn fixed_consts(alternatives: &Value, key: &str) -> BTreeSet<String> {
    alternatives
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|one| one["properties"][key]["const"].as_str().map(str::to_string))
        .collect()
}

/// The names the sentence opening with `lead` lists in backticks, in
/// the section `heading` opens.
pub fn names_after(text: &str, heading: &str, lead: &str) -> BTreeSet<String> {
    let sentence = sentence_after(&section(text, heading), lead)
        .unwrap_or_else(|| panic!("`{heading}` states its closed set after `{lead}`"));
    backticked(&sentence).into_iter().collect()
}

/// The rule codes `text` names in backticks, in the spelling the engine
/// publishes them by.
pub fn rule_codes_named(text: &str) -> Vec<String> {
    backticked(text)
        .into_iter()
        .filter(|span| {
            yunta_core::diagnostic::RuleCode::ALL
                .iter()
                .any(|code| code.as_str() == span)
        })
        .collect()
}
