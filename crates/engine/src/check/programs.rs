//! See [`super`]. The programs a workflow's literal commands start, for a
//! caller that can look them up where a run would.
//!
//! Reading a shell script without a shell is a heuristic, and a node that
//! runs earlier may install what a later one starts, so what this finds
//! is only ever a warning: a program the machine lacks is said before
//! the first token, and the run is the one that knows.

use super::*;

/// `(node, program)` for the program each simple command of a literal
/// `bash` `run:` or hook step starts, each pair once. A hook of
/// `node_defaults:` is attributed to that block. A word built from a
/// template or a variable, a path, and a shell builtin or keyword are
/// left out: none of them is a name to look up on `PATH`.
pub fn programs_named(workflow: &Workflow) -> Vec<(NodeId, String)> {
    let mut named: Vec<(NodeId, String)> = Vec::new();
    let mut name = |node: &NodeId, command: &str| {
        for program in leading_programs(command) {
            let entry = (node.clone(), program);
            if !named.contains(&entry) {
                named.push(entry);
            }
        }
    };
    for node in workflow.iter_nodes() {
        if let NodeKind::Bash { run } = &node.kind {
            name(&node.id, run);
        }
        if let Some(hooks) = &node.hooks {
            for step in hooks.before.iter().chain(&hooks.after) {
                name(&node.id, &step.run);
            }
        }
    }
    let defaults = workflow
        .node_defaults
        .as_ref()
        .and_then(|defaults| defaults.hooks.as_ref());
    if let Some(hooks) = defaults {
        for step in hooks.before.iter().chain(&hooks.after) {
            name(&NODE_DEFAULTS, &step.run);
        }
    }
    named
}

/// The first word of every simple command in `script`: split on lines
/// (a trailing `\` joins two), `;`, `&` and `|`, skipping the body of a
/// heredoc.
fn leading_programs(script: &str) -> Vec<String> {
    let mut programs = Vec::new();
    let mut heredoc: Option<String> = None;
    let joined = script.replace("\\\n", " ");
    for line in joined.lines() {
        if let Some(end) = &heredoc {
            if line.trim() == end {
                heredoc = None;
            }
            continue;
        }
        heredoc = heredoc_delimiter(line);
        for segment in line.split(['|', ';', '&']) {
            let first = segment.split_whitespace().find(|word| {
                !word.contains('=')
                    && !matches!(*word, "!" | "{" | "(" | "then" | "do" | "else" | "time")
            });
            if let Some(program) = first.map(|word| word.trim_start_matches(['(', '{'])) {
                if is_program(program) {
                    programs.push(program.to_string());
                }
            }
        }
    }
    programs
}

/// The word that ends a heredoc a line opens, if it opens one.
fn heredoc_delimiter(line: &str) -> Option<String> {
    let (_, rest) = line.split_once("<<")?;
    let word = rest
        .trim_start_matches('-')
        .split_whitespace()
        .next()?
        .trim_matches(['\'', '"']);
    (!word.is_empty()).then(|| word.to_string())
}

fn is_program(word: &str) -> bool {
    !word.is_empty()
        && !word.starts_with(['#', '-'])
        && !word.contains([
            '/', '$', '`', '{', '}', '<', '>', '(', ')', '"', '\'', '\\', '=',
        ])
        && !SHELL_WORDS.contains(&word)
}

/// Words a shell answers itself, so no `PATH` lookup is asked of them.
const SHELL_WORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "in", "function", "select", "time", "[", "[[", "]]", "test", "cd", "echo", "printf", "export",
    "set", "unset", "source", ".", "eval", "exec", "exit", "return", "read", "pwd", "true",
    "false", ":", "local", "shift", "trap", "wait", "command", "type", "alias", "ulimit", "umask",
    "break", "continue", "declare", "readonly", "let", "builtin", "hash", "jobs", "kill",
    "getopts",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_simple_command_names_its_program() {
        assert_eq!(
            leading_programs("git push -u origin {{run.branch}}\ngh pr create --fill"),
            vec!["git", "gh"]
        );
        assert_eq!(
            leading_programs("cargo fmt && cargo clippy -- -D warnings || make lint | tee out"),
            vec!["cargo", "cargo", "make", "tee"]
        );
        assert_eq!(
            leading_programs("git push -u \\\n  origin main"),
            vec!["git"],
            "a continued line is one command"
        );
    }

    #[test]
    fn what_no_path_lookup_can_answer_is_left_out() {
        assert_eq!(
            leading_programs("cd crates && FOO=1 ./scripts/check.sh; $TOOL run; {{inputs.cmd}}"),
            Vec::<String>::new()
        );
        assert_eq!(
            leading_programs("if true; then echo ok; fi"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_heredoc_body_is_text_not_commands() {
        assert_eq!(
            leading_programs("cat <<'EOF' > notes.md\nsomething to say\nEOF\nwc -l notes.md"),
            vec!["cat", "wc"]
        );
    }
}
