//! What a command proves about the files it names: whether it runs a
//! file at all, and whether it only finds a name in one.
//!
//! A criterion is read as the shell reads it: segments joined by `&&`,
//! `||` or `;`, each a program and its words. A segment that pipes reads
//! what another program printed, so what it finds is behavior; one that
//! reads a file the task writes passes once the task writes a name in it,
//! whatever the code does.

use crate::Task;

/// Whether `cmd` names the file at `path`: its whole path, its name, or
/// its name without the extension as a word of its own — how a test
/// runner is told which file to run (`--test pack_cmd`).
pub fn names_file(cmd: &str, path: &str) -> bool {
    let path = path.trim_start_matches("./");
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.split('.').next().unwrap_or(name);
    [path, name, stem]
        .into_iter()
        .filter(|said| !said.is_empty())
        .any(|said| as_a_word(cmd, said))
}

/// Each criterion of `task` that passes once a name is written in a file
/// the task changes, with that file: a `grep` or `rg` reading the file
/// rather than a pipe, or a `test -f`/`-e` of it that nothing runs after.
pub fn passes_by_a_name(task: &Task) -> Vec<(String, String)> {
    let changed: Vec<String> = task
        .changes
        .iter()
        .map(|change| crate::in_repo(change.file()))
        .collect();
    let mut found = Vec::new();
    for criterion in task
        .criteria
        .iter()
        .filter(|criterion| !criterion.is_guard())
    {
        let segments = segments(&criterion.cmd);
        for (at, segment) in segments.iter().enumerate() {
            let words: Vec<&str> = segment.split_whitespace().collect();
            let Some(file) = changed
                .iter()
                .find(|file| words.iter().any(|word| crate::in_repo(word) == **file))
            else {
                continue;
            };
            let presence = match words.first().copied() {
                Some("grep" | "egrep" | "fgrep" | "rg") => !segment.contains('|'),
                Some("test" | "[") => {
                    words.iter().any(|word| matches!(*word, "-f" | "-e" | "-s"))
                        && !segments
                            .iter()
                            .skip(at + 1)
                            .any(|later| later.contains(file.as_str()))
                }
                _ => false,
            };
            if presence {
                found.push((criterion.cmd.clone(), file.clone()));
            }
        }
    }
    found
}

/// The shell segments of `cmd`: what `&&`, `||` and `;` separate.
fn segments(cmd: &str) -> Vec<String> {
    cmd.replace("&&", ";")
        .replace("||", ";")
        .split([';', '\n'])
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Whether `said` appears in `text` with no word character on either
/// side.
fn as_a_word(text: &str, said: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(said).any(|(at, _)| {
        let before = text.get(..at).and_then(|head| head.chars().next_back());
        let after = text
            .get(at + said.len()..)
            .and_then(|rest| rest.chars().next());
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runner_names_a_file_by_its_stem() {
        assert!(names_file(
            "cargo test -p yunta --test pack_cmd",
            "crates/cli/tests/pack_cmd.rs"
        ));
        assert!(names_file("sh tests/a.sh", "./tests/a.sh"));
        assert!(!names_file(
            "cargo test --test pack_cmd_more",
            "tests/pack_cmd.rs"
        ));
        assert!(!names_file(
            "cargo test -p yunta --test pack_cmd",
            "crates/cli/tests/global_pack_scope_spec.rs"
        ));
    }

    fn task(criteria: &str) -> Task {
        crate::yaml::parse(&format!(
            "id: t\ntitle: T\nscope: [src/a.rs, run.sh]\ncriteria: {criteria}\n\
             changes:\n  - {{ at: src/a.rs::A, what: a }}\n  - {{ at: run.sh, what: b }}\n"
        ))
        .unwrap()
    }

    #[test]
    fn a_criterion_that_reads_a_name_in_a_changed_file_passes_by_a_name() {
        let found = passes_by_a_name(&task("[{ cmd: \"cargo test && grep -q case src/a.rs\" }]"));
        assert_eq!(
            found,
            [(
                "cargo test && grep -q case src/a.rs".to_string(),
                "src/a.rs".to_string()
            )]
        );
        assert_eq!(
            passes_by_a_name(&task("[{ cmd: \"test -f run.sh\" }]")).len(),
            1
        );
    }

    #[test]
    fn a_criterion_that_runs_what_it_checks_or_reads_what_ran_is_behavior() {
        for cmd in [
            "./bin/a | grep -q ok",
            "test -f run.sh && sh run.sh",
            "grep -q case src/other.rs",
        ] {
            let found = passes_by_a_name(&task(&format!("[{{ cmd: \"{cmd}\" }}]")));
            assert!(found.is_empty(), "`{cmd}` runs behavior: {found:?}");
        }
    }
}
