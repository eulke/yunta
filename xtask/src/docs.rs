//! `cargo xtask docs --check | --about [<paths>]` — the agent docs and the map
//! that routes a session to them.
//!
//! `AGENTS.md` maps where to read before a change and where knowledge goes.
//! `.agents/` holds the five files that say what the code cannot: `README.md`,
//! `architecture.md`, `choices.md`, `open.md` and `traps.md`. `--check` holds
//! them to their shapes ([`checks`]): the exact tree, the byte budgets, the
//! paths and globs they name, each rule's tags, the map's terms in the module
//! docs it points to, the crate table against the manifests, each crate
//! root's map of its modules, the `Reached by:` notes of the tests, and the
//! separation from the user docs. `--about` prints, for the files named or
//! for the diff, the module chain and the rules, traps, open work and tests
//! that reach those paths ([`about`](mod@about)).

mod about;
mod agents;
mod checks;
mod repo;

use repo::Repo;

/// `cargo xtask docs --check`: every problem the docs hold, or that they
/// hold. What they record about a file is `--about`'s to print.
pub fn check() -> Result<(), String> {
    let repo = Repo::open()?;
    let docs = agents::Docs::read(&repo)?;
    let problems = checks::all(&repo, &docs)?;
    if !problems.is_empty() {
        return Err(format!(
            "the agent docs do not hold — fix each, rewriting in place:\n{}",
            problems
                .iter()
                .map(|problem| format!("  {problem}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    emit("docs: the agent docs hold\n")
}

/// `cargo xtask docs --about [<paths>]`: the digest of `paths`, each read from
/// where the command runs, or of the diff when none is named. With neither,
/// it asks for the files.
pub fn about(paths: &[String]) -> Result<(), String> {
    let repo = Repo::open()?;
    let docs = agents::Docs::read(&repo)?;
    let named = match paths.is_empty() {
        true => repo.changed()?,
        false => paths
            .iter()
            .map(|path| repo.relative(path))
            .collect::<Result<Vec<_>, _>>()?,
    };
    if named.is_empty() {
        return Err("nothing changed yet: name the files you will change, \
             `cargo xtask docs --about <files>`"
            .to_string());
    }
    emit(&about::digest(&repo, &docs, &named)?)
}

/// Writes `text` to stdout. A reader that stops early, as `| head` does, has
/// what it asked for, so a closed pipe is no failure.
fn emit(text: &str) -> Result<(), String> {
    use std::io::Write;
    match std::io::stdout().lock().write_all(text.as_bytes()) {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        written => written.map_err(|error| format!("cannot write to stdout: {error}")),
    }
}
