//! See [`super`]. The programs a workflow's literal commands start, for a
//! caller that can look them up where a run would.
//!
//! Reading a shell script without a shell is a heuristic, and a node that
//! runs earlier may install what a later one starts, so what this finds
//! is only ever a warning: a program the machine lacks is said before
//! the first token, and the run is the one that knows.

use super::*;

/// `(node, program)` for the program each simple command of a `bash`
/// `run:` or hook step starts, each pair once — a project's command read
/// as `config` declares it. A hook of `node_defaults:` is attributed to
/// that block. A word built from a template or a variable, a path, and a
/// shell builtin or keyword are left out: none of them is a name to look
/// up on `PATH`.
pub fn programs_named(workflow: &Workflow, config: &ConfigLayer) -> Vec<(NodeId, String)> {
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
        for command in yunta_core::node_commands(node).filter_map(|run| run.text(config)) {
            name(&node.id, command);
        }
    }
    let defaults = workflow
        .node_defaults
        .as_ref()
        .and_then(|defaults| defaults.hooks.as_ref());
    if let Some(hooks) = defaults {
        for step in hooks.before.iter().chain(&hooks.after) {
            if let Some(command) = step.run.text(config) {
                name(&NODE_DEFAULTS, command);
            }
        }
    }
    named
}

/// The first word of every simple command in `script`, as a shell splits
/// the script into commands ([`yunta_core::shell`]).
pub(crate) fn leading_programs(script: &str) -> Vec<String> {
    yunta_core::shell::simple_commands(script)
        .iter()
        .filter_map(|command| {
            let first = command.split_whitespace().find(|word| {
                !word.contains('=')
                    && !matches!(*word, "!" | "{" | "(" | "then" | "do" | "else" | "time")
            })?;
            let program = first.trim_start_matches(['(', '{']);
            is_program(program).then(|| program.to_string())
        })
        .collect()
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
    fn a_quoted_line_or_a_redirection_starts_no_program() {
        assert_eq!(
            leading_programs("printf 'tasks:\n  - id: T001\n    scope: [a.txt]\n' > tasks.yaml"),
            Vec::<String>::new()
        );
        assert_eq!(
            leading_programs("cargo test 2>&1 | tail -5"),
            vec!["cargo", "tail"]
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
