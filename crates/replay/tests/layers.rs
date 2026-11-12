//! The crate's modules stand in layers, and each names only the ones beneath
//! it: this reads `src/` to say so when a `use` crosses a layer upwards.

use std::path::{Path, PathBuf};

/// The crate's modules from the bottom. A file belongs to the public module
/// its path starts with, and every other file to replay, at the root.
const LAYERS: [&[&str]; 5] = [
    &["modes", "registry"],
    &["replay"],
    &["stats"],
    &["history"],
    &["receipt", "verification_effectiveness", "frame"],
];

/// What replay folds never depends on what is read off it: each module names,
/// through `crate::`, only modules in a layer beneath its own.
#[test]
fn each_module_names_only_the_modules_beneath_it() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut crossings = Vec::new();
    for file in rust_files(&src) {
        let relative = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let module = module_of(&relative);
        let code = without_comments(&std::fs::read_to_string(&file).unwrap());
        for named in named_modules(&code) {
            if named != module && layer_of(named) >= layer_of(module) {
                crossings.push(format!("{relative}, in {module}, names {named}"));
            }
        }
    }
    crossings.sort();
    crossings.dedup();
    assert!(
        crossings.is_empty(),
        "a module names one beside or above its own layer; what both need belongs beneath both:\n{}",
        crossings.join("\n")
    );
}

/// Every `.rs` file under `dir`.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files
}

/// The module a file under `src/` belongs to.
fn module_of(relative: &str) -> &'static str {
    let first = relative.split('/').next().unwrap().trim_end_matches(".rs");
    public(first).unwrap_or("replay")
}

/// The public module named `name`, as `LAYERS` spells it.
fn public(name: &str) -> Option<&'static str> {
    LAYERS
        .iter()
        .flat_map(|layer| layer.iter())
        .copied()
        .find(|module| *module != "replay" && *module == name)
}

fn layer_of(module: &str) -> usize {
    LAYERS
        .iter()
        .position(|layer| layer.contains(&module))
        .unwrap_or_else(|| panic!("`{module}` stands in no layer of `LAYERS`"))
}

/// `code` with its `//` comments taken out: a doc link is no dependency.
fn without_comments(code: &str) -> String {
    code.lines()
        .map(|line| line.split("//").next().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The modules `code` names through `crate::`: a public module by its name,
/// and anything else, an item of the root, as replay.
fn named_modules(code: &str) -> Vec<&'static str> {
    let mut named = Vec::new();
    for (at, _) in code.match_indices("crate::") {
        if code[..at].ends_with('$') {
            continue;
        }
        let rest = &code[at + "crate::".len()..];
        let firsts = match rest.strip_prefix('{') {
            Some(list) => top_level_items(list),
            None => vec![leading_ident(rest)],
        };
        named.extend(
            firsts
                .into_iter()
                .map(|first| public(first).unwrap_or("replay")),
        );
    }
    named
}

/// The first name of each item of a `{…}` list that `list` opens, at its own depth.
fn top_level_items(list: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (at, character) in list.char_indices() {
        match character {
            '{' => depth += 1,
            '}' if depth == 0 => {
                items.push(leading_ident(list[start..at].trim_start()));
                break;
            }
            '}' => depth -= 1,
            ',' if depth == 0 => {
                items.push(leading_ident(list[start..at].trim_start()));
                start = at + 1;
            }
            _ => {}
        }
    }
    items.into_iter().filter(|item| !item.is_empty()).collect()
}

fn leading_ident(text: &str) -> &str {
    let end = text
        .find(|character: char| !(character.is_alphanumeric() || character == '_'))
        .unwrap_or(text.len());
    &text[..end]
}
