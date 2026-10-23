//! What `docs --check` holds the agent docs to. Each check returns its
//! problems as `<file>:<line>: what is wrong — how to fix it`; no problem is a
//! pass. The rules a check holds are the ones the map states, so a doc that
//! drifts from the code, grows into a log or leaks into the user docs fails
//! here instead of misleading the next session.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use regex::Regex;

use super::agents::{self, Docs, Tag, AGENT_DOCS, MAP, RULE_SETS};
use super::repo::Repo;

/// The byte budget of each doc: past it, a doc carries what the code, a test
/// or a commit message should.
const BUDGETS: [(&str, usize); 6] = [
    (MAP, 8192),
    (".agents/README.md", 1024),
    (".agents/architecture.md", 6144),
    (".agents/choices.md", 6144),
    (".agents/open.md", 8192),
    (".agents/traps.md", 4096),
];

/// How many traps the traps file keeps: past eight it is a log, and the oldest
/// trap is owed a check.
const MOST_TRAPS: usize = 8;

/// The roots a backticked path in a doc points into the tree from. A skills
/// root under `.agents/` is a CLI's, never a doc's pointer, so it is not one.
const POINTED_ROOTS: [&str; 7] = [
    "crates/", "xtask/", "packs/", "docs/", ".github/", "scripts/", ".agents/",
];

/// Every problem the docs hold, check by check.
pub(super) fn all(repo: &Repo, docs: &Docs) -> Result<Vec<String>, String> {
    let mut problems = tree(repo)?;
    problems.extend(budgets(docs));
    problems.extend(shapes(docs));
    problems.extend(pointers(repo, docs)?);
    problems.extend(rule_tags(repo, docs)?);
    problems.extend(map_terms(repo, docs));
    problems.extend(crate_table(repo, docs)?);
    problems.extend(crate_maps(repo)?);
    problems.extend(reach_notes(repo)?);
    problems.extend(separation(repo)?);
    problems.extend(instructions(repo));
    Ok(problems)
}

/// `.agents/` holds exactly the five docs, each a regular file, beside a
/// skills root a CLI loads if there is one.
fn tree(repo: &Repo) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    let indexed = repo.modes_under(".agents")?;
    let present: BTreeSet<&str> = repo
        .tracked()
        .iter()
        .map(String::as_str)
        .filter(|path| path.starts_with(".agents/") && !is_skills_root(path))
        .collect();
    for path in &present {
        if !AGENT_DOCS.contains(path) {
            problems.push(format!(
                "{path}: is not one of the five agent docs — fold it into the one whose subject it is, or move it out of `.agents/`"
            ));
        }
    }
    for (mode, path) in &indexed {
        if AGENT_DOCS.contains(&path.as_str()) && mode != "100644" {
            problems.push(format!(
                "{path}: is mode {mode} — an agent doc is a regular file"
            ));
        }
    }
    for doc in AGENT_DOCS {
        if !present.contains(doc) {
            problems.push(format!(
                "{doc}: is missing — the agent docs are these five files"
            ));
        }
    }
    Ok(problems)
}

/// Whether `path` is the skills root a CLI loads under `.agents/`, or below it:
/// the root itself may be a link, which git tracks as the path.
fn is_skills_root(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    path == ".agents/skills" || path.starts_with(".agents/skills/")
}

/// Each doc within its byte budget, and the traps within their count.
fn budgets(docs: &Docs) -> Vec<String> {
    let mut problems: Vec<String> = BUDGETS
        .iter()
        .filter_map(|(path, most)| {
            let size = docs.text(path)?.len();
            (size > *most).then(|| {
                format!("{path}: {size} bytes, past its {most} — cut what the code, a test or a commit already says")
            })
        })
        .collect();
    if let Some(traps) = docs.text(".agents/traps.md") {
        let count = agents::entries(traps).len();
        if count > MOST_TRAPS {
            problems.push(format!(
                ".agents/traps.md: {count} traps, past {MOST_TRAPS} — give one a check, or delete one no change meets any more"
            ));
        }
    }
    problems
}

/// The fields each kind of entry is written with.
fn shapes(docs: &Docs) -> Vec<String> {
    let shapes: [(&str, &[&str]); 2] = [
        (".agents/traps.md", &["Paths", "Wrong", "Right"]),
        (".agents/open.md", &["Paths"]),
    ];
    let mut problems = Vec::new();
    for (path, fields) in shapes {
        let Some(text) = docs.text(path) else {
            continue;
        };
        for entry in agents::entries(text) {
            for field in fields
                .iter()
                .filter(|field| !entry.fields.contains_key(**field))
            {
                problems.push(format!(
                    "{path}:{}: `{}` has no `{field}:` line — every entry here carries one",
                    entry.line, entry.title
                ));
            }
            if entry.fields.contains_key("Paths") && entry.paths().is_empty() {
                problems.push(format!(
                    "{path}:{}: `{}` has a `Paths:` line that names no path — write each as `<path>`",
                    entry.line, entry.title
                ));
            }
        }
    }
    problems
}

/// Every path a doc points into the tree with is there, every glob selects a
/// tracked file, every path a `Paths:` field names selects files wherever it
/// points, and every relative link resolves.
fn pointers(repo: &Repo, docs: &Docs) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for (path, text) in docs.each() {
        let selecting = selecting_spans(path, text);
        for (line, span) in agents::spans_by_line(text) {
            if span.contains(char::is_whitespace) {
                continue;
            }
            if selecting.contains(&span) {
                problems.extend(
                    selects_nothing(repo, &span).map(|why| format!("{path}:{line}: {why}")),
                );
                continue;
            }
            let pointed = POINTED_ROOTS.iter().any(|root| span.starts_with(root));
            if !pointed || is_skills_root(&span) {
                continue;
            }
            let found = match is_glob(&span) {
                true => repo.selected(&span).is_ok_and(|found| !found.is_empty()),
                false => repo.holds(&span),
            };
            if !found {
                problems.push(format!(
                    "{path}:{line}: `{span}` is in no tracked path — point at where it lives now, or drop it"
                ));
            }
        }
        for (line, target) in links(text) {
            let resolved = resolve(path, &target);
            if !repo.holds(&resolved) {
                problems.push(format!(
                    "{path}:{line}: the link to `{target}` leads nowhere"
                ));
            }
        }
    }
    Ok(problems)
}

/// The spans of the doc at `path` that select files for `--about`: what the
/// `Paths:` of its entries and of its rules name.
fn selecting_spans(path: &str, text: &str) -> BTreeSet<String> {
    let mut selecting = BTreeSet::new();
    if path == ".agents/traps.md" || path == ".agents/open.md" {
        selecting.extend(agents::entries(text).iter().flat_map(agents::Entry::paths));
    }
    for (doc, sections) in RULE_SETS {
        if doc == path {
            selecting.extend(
                agents::rules(text, sections)
                    .into_iter()
                    .flat_map(|rule| rule.paths),
            );
        }
    }
    selecting
}

/// Why `span` selects no file for `--about`, or `None` when it selects one: a
/// glob that matches a tracked file, or a tracked file itself. A directory
/// matches none of its files as itself, so it is written `<dir>/**`.
fn selects_nothing(repo: &Repo, span: &str) -> Option<String> {
    if is_glob(span) {
        return match repo.selected(span) {
            Err(why) => Some(why),
            Ok(found) => found.is_empty().then(|| {
                format!("`{span}` selects no tracked file — point at where that code lives now")
            }),
        };
    }
    match (repo.tracks(span), repo.holds(span)) {
        (true, _) => None,
        (false, true) => Some(format!(
            "`{span}` names a directory, which selects none of its files — write `{}/**`",
            span.trim_end_matches('/')
        )),
        (false, false) => Some(format!(
            "`{span}` is in no tracked path — point at where it lives now, or drop it"
        )),
    }
}

/// Every rule names what holds it, and every name resolves: a smell counter
/// the ratchet measures, a test target and test that exist, and paths for a
/// rule only a review holds.
fn rule_tags(repo: &Repo, docs: &Docs) -> Result<Vec<String>, String> {
    let counters: BTreeSet<String> = repo
        .read("xtask/smells.baseline")?
        .lines()
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect();
    let mut problems = Vec::new();
    for (path, sections) in RULE_SETS {
        let Some(text) = docs.text(path) else {
            continue;
        };
        for rule in agents::rules(text, sections) {
            let at = format!("{path}:{}", rule.line);
            if rule.tags.is_empty() {
                problems.push(format!(
                    "{at}: a rule with no tag — name what holds it: `[test <crate>/<target>]`, `[smell <counter>]` or `[review]`"
                ));
            }
            if rule.paths_said && rule.paths.is_empty() {
                problems.push(format!(
                    "{at}: its `Paths:` names no path — write each as `<path>`"
                ));
            }
            for tag in &rule.tags {
                problems.extend(
                    unresolved(repo, &counters, tag, rule.paths_said)
                        .map(|why| format!("{at}: {why}")),
                );
            }
        }
    }
    Ok(problems)
}

/// Why `tag` names nothing, or `None` when it resolves.
fn unresolved(
    repo: &Repo,
    counters: &BTreeSet<String>,
    tag: &Tag,
    has_paths: bool,
) -> Option<String> {
    match tag {
        Tag::Smell(name) => (!counters.contains(name))
            .then(|| format!("`[smell {name}]` names no counter the ratchet measures")),
        Tag::Review => (!has_paths).then(|| {
            "a rule only a review holds has no `Paths:` line — name where it bites, so `--about` prints it".to_string()
        }),
        Tag::Malformed(written) => Some(format!(
            "`[{written}]` names no crate and target — write `[test <crate>/<target>]` or `[test <crate>/<target>::<test>]`"
        )),
        Tag::Test { krate, target, item } => {
            let files = test_target(repo, krate, target);
            if files.is_empty() {
                return Some(format!("`[test {krate}/{target}]` names no test target"));
            }
            let item = item.as_ref()?;
            let found = files.iter().any(|file| {
                repo.read_if_there(file)
                    .is_some_and(|text| defines_test(&text, item))
            });
            (!found).then(|| format!("`[test {krate}/{target}::{item}]` names no test in that target"))
        }
    }
}

/// The files of the test target `target` of the crate `krate`, as cargo finds
/// a target: `tests/<target>.rs` and the modules under `tests/<target>/`, or
/// `tests/<target>/main.rs` and the files beside it. A directory with neither
/// is a helper, never a target.
fn test_target<'a>(repo: &'a Repo, krate: &str, target: &str) -> Vec<&'a str> {
    if target.contains('/') {
        return Vec::new();
    }
    let dir = match krate {
        "xtask" => "xtask".to_string(),
        _ => format!("crates/{krate}"),
    };
    let file = format!("{dir}/tests/{target}.rs");
    let nested = format!("{dir}/tests/{target}/");
    if !repo.tracks(&file) && !repo.tracks(&format!("{nested}main.rs")) {
        return Vec::new();
    }
    repo.tracked()
        .iter()
        .map(String::as_str)
        .filter(|path| *path == file || path.starts_with(&nested))
        .collect()
}

/// Whether `text` defines `item` as a test: a function of that name whose
/// attributes mark it one (`#[test]`, `#[tokio::test]` and their kin), not a
/// helper beside the tests.
fn defines_test(text: &str, item: &str) -> bool {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    lines.iter().enumerate().any(|(at, line)| {
        let declared = ["fn ", "async fn ", "pub fn ", "pub async fn "]
            .iter()
            .any(|opening| line.starts_with(&format!("{opening}{item}(")));
        declared
            && lines
                .iter()
                .take(at)
                .rev()
                .take_while(|line| line.starts_with("#[") || line.starts_with("//"))
                .any(|line| test_attribute().is_match(line))
    })
}

fn test_attribute() -> &'static Regex {
    static TEST: OnceLock<Regex> = OnceLock::new();
    TEST.get_or_init(|| {
        Regex::new(r"^#\[(?:[a-z_]+::)*test(?:\(.*\))?\]$").expect("a valid pattern")
    })
}

/// Each term a row of the map names is in the module doc of a file the row
/// opens: the map sends a session there, and the `//!` it reads says it.
fn map_terms(repo: &Repo, docs: &Docs) -> Vec<String> {
    let Some(map) = docs.text(MAP) else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    for (line, cells) in agents::table(map, "You are changing") {
        let (Some(terms), Some(files)) = (cells.first(), cells.get(1)) else {
            continue;
        };
        let rust: Vec<String> = agents::spans(files)
            .into_iter()
            .filter(|file| file.ends_with(".rs"))
            .collect();
        if rust.is_empty() {
            continue;
        }
        let told: String = rust
            .iter()
            .filter_map(|file| repo.read_if_there(file))
            .map(|text| module_doc(&text))
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        for term in terms.split(", ").map(str::trim) {
            if !names(&told, term) {
                problems.push(format!(
                    "{MAP}:{line}: no module doc the row opens names `{term}` — say it in the `//!` of the file it sends a session to"
                ));
            }
        }
    }
    problems
}

/// Whether `told` names `term` as whole words, in its plural or its singular.
fn names(told: &str, term: &str) -> bool {
    let term = term.to_lowercase();
    let says = |words: &str| {
        Regex::new(&format!(r"\b{}\b", regex::escape(words)))
            .is_ok_and(|words| words.is_match(told))
    };
    says(&term) || term.strip_suffix('s').is_some_and(says)
}

/// The architecture's crate table says what each workspace member depends on,
/// as its manifest's `[dependencies]` do, and has a row for every member.
fn crate_table(repo: &Repo, docs: &Docs) -> Result<Vec<String>, String> {
    let Some(architecture) = docs.text(".agents/architecture.md") else {
        return Ok(Vec::new());
    };
    let members = members(repo)?;
    let mut problems = Vec::new();
    let mut rowed = BTreeSet::new();
    for (line, cells) in agents::table(architecture, "Crate") {
        let at = format!(".agents/architecture.md:{line}");
        let Some(name) = cells
            .first()
            .and_then(|cell| agents::spans(cell).into_iter().next())
        else {
            continue;
        };
        let Some(depends) = members.get(&name) else {
            problems.push(format!("{at}: `{name}` is no workspace member"));
            continue;
        };
        rowed.insert(name.clone());
        let said: BTreeSet<String> = cells
            .get(2)
            .map(|cell| said_dependencies(cell))
            .unwrap_or_default();
        if &said != depends {
            problems.push(format!(
                "{at}: `{name}` depends on {} — write that in its row",
                listed(depends)
            ));
        }
    }
    for member in members.keys().filter(|member| !rowed.contains(*member)) {
        problems.push(format!(
            ".agents/architecture.md: the crate table has no row for `{member}`"
        ));
    }
    Ok(problems)
}

/// The workspace members by package name, each with the members its
/// `[dependencies]` name.
fn members(repo: &Repo) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let manifests: Vec<(String, String)> = repo
        .tracked()
        .iter()
        .filter(|path| {
            path.as_str() == "xtask/Cargo.toml"
                || (path.starts_with("crates/")
                    && path.ends_with("/Cargo.toml")
                    && path.matches('/').count() == 2)
        })
        .map(|path| Ok((package_name(&repo.read(path)?), repo.read(path)?)))
        .collect::<Result<_, String>>()?;
    let names: BTreeSet<String> = manifests.iter().map(|(name, _)| name.clone()).collect();
    Ok(manifests
        .iter()
        .map(|(name, text)| {
            let depends = dependency_keys(text)
                .into_iter()
                .filter(|key| names.contains(key))
                .collect();
            (name.clone(), depends)
        })
        .collect())
}

fn package_name(manifest: &str) -> String {
    manifest
        .lines()
        .find_map(|line| line.strip_prefix("name = "))
        .map(|name| name.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

/// The keys of a manifest's `[dependencies]` table.
fn dependency_keys(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, _)| key.trim().trim_end_matches(".workspace").to_string())
        .collect()
}

/// The members a row's dependency cell names: `nothing in the workspace`, or
/// short names, `core` for `yunta-core` and `yunta` for the CLI.
fn said_dependencies(cell: &str) -> BTreeSet<String> {
    if cell.contains("nothing") {
        return BTreeSet::new();
    }
    cell.split(',')
        .map(|name| name.trim().trim_matches('`'))
        .filter(|name| !name.is_empty())
        .map(|name| match name {
            "yunta" | "xtask" => name.to_string(),
            short => format!("yunta-{short}"),
        })
        .collect()
}

fn listed(names: &BTreeSet<String>) -> String {
    match names.is_empty() {
        true => "nothing in the workspace".to_string(),
        false => names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Each crate root's `//!` maps its crate: it names every module the root
/// declares.
fn crate_maps(repo: &Repo) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for root in crate_roots(repo) {
        let text = repo.read(root)?;
        let map = module_doc(&text);
        for module in declared_modules(&text) {
            if !map.contains(&format!("`{module}`")) {
                problems.push(format!(
                    "{root}: the crate's map names no `{module}` — say in its `//!` what the module holds"
                ));
            }
        }
    }
    Ok(problems)
}

/// The root of every crate: `lib.rs` where there is one, else `main.rs`.
fn crate_roots(repo: &Repo) -> Vec<&str> {
    let tracked = repo.tracked();
    tracked
        .iter()
        .map(String::as_str)
        .filter(|path| {
            let lib = path.ends_with("/src/lib.rs");
            let main = path.ends_with("/src/main.rs")
                && !tracked
                    .iter()
                    .any(|other| *other == path.replace("main.rs", "lib.rs"));
            (path.starts_with("crates/") || path.starts_with("xtask/"))
                && path.matches('/').count() == if path.starts_with("xtask/") { 2 } else { 3 }
                && (lib || main)
        })
        .collect()
}

/// The modules a root declares at its top level, its tests aside.
fn declared_modules(text: &str) -> Vec<String> {
    let mut modules = Vec::new();
    let mut after_test_cfg = false;
    for line in text.lines() {
        let declared = line
            .strip_prefix("pub mod ")
            .or_else(|| line.strip_prefix("pub(crate) mod "))
            .or_else(|| line.strip_prefix("mod "))
            .and_then(|rest| rest.strip_suffix(';'));
        if let (Some(name), false) = (declared, after_test_cfg) {
            modules.push(name.trim().to_string());
        }
        after_test_cfg = line.trim() == "#[cfg(test)]";
    }
    modules
}

/// Every `Reached by:` note names paths that are there: the note says what
/// code can break the rule its test holds, and `--about` prints the test for
/// that code.
fn reach_notes(repo: &Repo) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for file in rust_files(repo) {
        let text = repo.read(file)?;
        for note in reach(&text) {
            if note.paths.is_empty() {
                problems.push(format!("{file}:{}: `Reached by:` names no path", note.line));
            }
            for path in &note.paths {
                if let Some(why) = selects_nothing(repo, path) {
                    problems.push(format!("{file}:{}: `Reached by:` {why}", note.line));
                }
            }
        }
    }
    Ok(problems)
}

/// A test's note of the code that can break the rule it holds.
pub(super) struct Reach {
    pub(super) line: usize,
    /// The test the note documents: the next function after it.
    pub(super) test: String,
    pub(super) paths: Vec<String>,
}

/// The `Reached by:` notes of `text`: a comment line that opens with the
/// words, its spans and those of the comment lines that follow it.
pub(super) fn reach(text: &str) -> Vec<Reach> {
    let lines: Vec<&str> = text.lines().collect();
    let mut notes = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let comment = line.trim_start();
        let said = comment.trim_start_matches(['/', '!']).trim_start();
        let Some(rest) = said.strip_prefix("Reached by:") else {
            continue;
        };
        if !comment.starts_with("//") {
            continue;
        }
        let mut paths = agents::spans(rest);
        let mut next = at + 1;
        while let Some(more) = lines.get(next).map(|line| line.trim_start()) {
            if !more.starts_with("//") {
                break;
            }
            paths.extend(agents::spans(more));
            next += 1;
        }
        let test = lines
            .iter()
            .skip(next)
            .find_map(|line| {
                line.trim_start()
                    .split("fn ")
                    .nth(1)?
                    .split('(')
                    .next()
                    .map(str::to_string)
            })
            .unwrap_or_default();
        notes.push(Reach {
            line: at + 1,
            test,
            paths,
        });
    }
    notes
}

/// Every Rust file the workspace tracks.
pub(super) fn rust_files(repo: &Repo) -> Vec<&str> {
    repo.tracked()
        .iter()
        .map(String::as_str)
        .filter(|path| {
            (path.starts_with("crates/") || path.starts_with("xtask/")) && path.ends_with(".rs")
        })
        .collect()
}

/// The user docs never name an agent doc: a person reads what the product
/// does, and the agent docs stay out of their way.
fn separation(repo: &Repo) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    let user_docs = repo.tracked().iter().map(String::as_str).filter(|path| {
        *path == "README.md"
            || (path.starts_with("docs/")
                && path.ends_with(".md")
                && !path.starts_with("docs/design/"))
    });
    for doc in user_docs {
        for (at, line) in repo.read(doc)?.lines().enumerate() {
            if crate::smells::names_an_agent_doc(line) {
                problems.push(format!(
                    "{doc}:{}: a user doc names an agent doc — say it for the person, or leave it to `.agents/`",
                    at + 1
                ));
            }
        }
    }
    Ok(problems)
}

/// `CLAUDE.md` is the one line that loads the map, and the layer a run distils
/// knowledge into stays out of the tree.
fn instructions(repo: &Repo) -> Vec<String> {
    let mut problems = Vec::new();
    if repo.read_if_there("CLAUDE.md").as_deref() != Some("@AGENTS.md\n") {
        problems.push(
            "CLAUDE.md: is not the one line `@AGENTS.md` — Claude Code loads the map through it, and anything else beside it is a second map"
                .to_string(),
        );
    }
    let ignores_knowledge = repo
        .read_if_there(".gitignore")
        .is_some_and(|text| text.lines().any(|line| line.trim() == "/.yunta/knowledge/"));
    if !ignores_knowledge {
        problems.push(
            ".gitignore: holds no `/.yunta/knowledge/` — what a run distils is the run's, never a doc of the tree".to_string(),
        );
    }
    problems
}

/// The leading `//!` lines of a Rust file, their markers taken off.
pub(super) fn module_doc(text: &str) -> String {
    text.lines()
        .map(str::trim_start)
        .skip_while(|line| line.is_empty() || line.starts_with("#!["))
        .take_while(|line| line.starts_with("//!"))
        .map(|line| line.trim_start_matches("//!").trim())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn is_glob(span: &str) -> bool {
    span.contains(['*', '?', '[', '{'])
}

/// The relative link targets of a markdown text, each with its line.
fn links(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .flat_map(|(at, line)| {
            line.split("](")
                .skip(1)
                .filter_map(|rest| rest.split(')').next())
                .map(|target| target.split('#').next().unwrap_or_default().to_string())
                .filter(|target| {
                    !target.is_empty() && !target.contains("://") && !target.starts_with("mailto:")
                })
                .map(move |target| (at + 1, target))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `target` read from the directory of the doc at `doc`.
fn resolve(doc: &str, target: &str) -> String {
    let mut at: Vec<&str> = doc.split('/').collect();
    at.pop();
    for part in target.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                at.pop();
            }
            name => at.push(name),
        }
    }
    at.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_term_counts_in_its_plural_or_its_singular() {
        assert!(names("each event kind is a name", "event kinds"));
        assert!(names("the payload fields of a kind", "payload fields"));
        assert!(!names("nothing about it", "config keys"));
    }

    #[test]
    fn a_term_counts_only_as_whole_words() {
        assert!(names("all four checks run in ci.", "CI"));
        assert!(!names("the decision index", "CI"));
        assert!(!names("it may delegate the work", "gates"));
    }

    #[test]
    fn a_test_is_a_function_its_attributes_mark_as_one() {
        let text = "/// The rule.\n#[tokio::test(flavor = \"multi_thread\")]\nasync fn holds() {}\n\nfn helper() {}\n#[cfg(test)]\nfn gated() {}\n    #[test]\n    fn inside(log in logs()) {}\n";
        assert!(defines_test(text, "holds"));
        assert!(defines_test(text, "inside"));
        assert!(!defines_test(text, "helper"));
        assert!(!defines_test(text, "gated"));
        assert!(!defines_test(text, "absent"));
    }

    #[test]
    fn a_root_declares_its_modules_and_its_tests_stay_aside() {
        let root = "//! The crate.\n\npub mod events;\nmod glob;\npub(crate) mod scan;\n#[cfg(test)]\nmod tests;\n";
        assert_eq!(declared_modules(root), ["events", "glob", "scan"]);
    }

    #[test]
    fn a_reach_note_names_its_paths_and_its_test() {
        let text = "/// The rule.\n/// Reached by: `crates/a/src/**`,\n/// `crates/b/src/x.rs`\n#[test]\nfn the_rule_holds() {}\n";
        let notes = reach(text);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].test, "the_rule_holds");
        assert_eq!(notes[0].paths, ["crates/a/src/**", "crates/b/src/x.rs"]);
    }

    #[test]
    fn a_link_resolves_from_its_doc_and_a_dependency_cell_reads_short_names() {
        assert_eq!(resolve(".agents/open.md", "../crates/a.rs"), "crates/a.rs");
        assert_eq!(resolve("AGENTS.md", "CONTRIBUTING.md"), "CONTRIBUTING.md");
        assert_eq!(
            said_dependencies("core, storage"),
            BTreeSet::from(["yunta-core".to_string(), "yunta-storage".to_string()])
        );
        assert!(said_dependencies("nothing in the workspace").is_empty());
    }
}
