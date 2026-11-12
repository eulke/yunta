//! No public item of the API names an engine, store or adapter type. An
//! interface reads what the API returns and hands in what it asks for, and
//! links none of the crates below it: a public item that named one of their
//! types would hand an interface a value it cannot spell, or make it link
//! the crate after all.
//!
//! Read off the source: every `pub` item outside a test module (a function's
//! signature, a type, a field, a constant, a re-export) is held against the
//! paths of the crates below the API and the names its file imports from
//! them. `pub(crate)` and `pub(super)` items are the API's own.

use std::path::Path;

/// The crates below the API, as code names them.
const BELOW: [&str; 3] = ["yunta_engine", "yunta_storage", "yunta_adapters"];

#[test]
fn no_public_item_of_the_api_names_a_type_below_it() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut named = Vec::new();
    for file in rust_files(&src) {
        let text = std::fs::read_to_string(&file).expect("a source file reads");
        for item in naming_below(&text) {
            named.push(format!(
                "{}: {item}",
                file.strip_prefix(&src).unwrap().display()
            ));
        }
    }
    assert!(
        named.is_empty(),
        "these public items of yunta-api name a type of a crate below it; hand an interface \
         core, replay or render data, or the API's own type, and keep the item `pub(crate)` \
         when only the API uses it:\n{}",
        named.join("\n")
    );
}

/// A public item planted in a file is found, through the path it writes and
/// through a name its file imports; a crate-visible one and one in a test
/// module are not.
#[test]
fn a_public_item_naming_a_type_below_is_found() {
    let planted = "\
use yunta_engine::CheckoutPool;
use yunta_storage::{AsyncStorage, Storage};

pub fn pool() -> CheckoutPool {}
pub struct Held {
    pub storage: Storage,
    store: AsyncStorage,
}
pub(crate) fn own() -> CheckoutPool {}
pub fn report() -> yunta_engine::RunReport {}
pub fn reading() -> yunta_replay::RunState {}

#[cfg(test)]
mod tests {
    pub fn helper() -> CheckoutPool {}
}
";
    let found = naming_below(planted);
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(found[0].starts_with("pub fn pool()"), "{found:?}");
    assert!(found[1].starts_with("pub storage: Storage"), "{found:?}");
    assert!(found[2].starts_with("pub fn report()"), "{found:?}");
}

/// Every public item of `text` whose signature names a crate below the API,
/// by path or by a name the file imports from one, each by its first line.
fn naming_below(text: &str) -> Vec<String> {
    let imported = imported_below(text);
    let names_below = |signature: &str| {
        BELOW
            .iter()
            .any(|krate| signature.contains(&format!("{krate}::")))
            || imported.iter().any(|name| mentions(signature, name))
    };
    let production = text.split("#[cfg(test)]").next().unwrap_or(text);
    let lines: Vec<&str> = production.lines().collect();
    let mut found = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("pub ") {
            continue;
        }
        let signature = signature_from(&lines[at..]);
        if names_below(&signature) {
            found.push(trimmed.to_string());
        }
    }
    found
}

/// The signature an item opens at the first of `lines`: its lines up to the
/// body it opens or the end it reaches.
fn signature_from(lines: &[&str]) -> String {
    let mut signature = String::new();
    for line in lines {
        signature.push_str(line);
        signature.push(' ');
        let code = line.trim_end();
        if code.ends_with('{') || code.ends_with(';') || code.ends_with(',') {
            break;
        }
    }
    signature
}

/// The names `text` imports from a crate below the API.
fn imported_below(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut statement = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if statement.is_empty() && !trimmed.starts_with("use ") {
            continue;
        }
        statement.push_str(trimmed);
        if !trimmed.ends_with(';') {
            continue;
        }
        if BELOW
            .iter()
            .any(|krate| statement.starts_with(&format!("use {krate}::")))
        {
            names.extend(names_in(&statement));
        }
        statement.clear();
    }
    names
}

/// The names a `use` statement brings in: the last segment of each path it
/// lists, or what it renames one to; `self` and globs bring none.
fn names_in(statement: &str) -> Vec<String> {
    let body = statement.trim_start_matches("use ").trim_end_matches(';');
    let listed = match (body.find('{'), body.rfind('}')) {
        (Some(open), Some(close)) => body[open + 1..close].to_string(),
        _ => body.to_string(),
    };
    listed
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .filter_map(|path| {
            let name = match path.split_once(" as ") {
                Some((_, renamed)) => renamed,
                None => path.rsplit("::").next().unwrap_or(path),
            };
            let name = name.trim_matches(|c| c == '{' || c == '}').trim();
            (name != "self" && name != "*" && !name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

/// Whether `text` mentions `name` as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

/// Every `.rs` file under `dir` but a test module's, in a stable order.
fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(at) = pending.pop() {
        for entry in std::fs::read_dir(&at).expect("a source directory reads") {
            let path = entry.expect("an entry reads").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && path.file_name().is_some_and(|name| name != "tests.rs")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
