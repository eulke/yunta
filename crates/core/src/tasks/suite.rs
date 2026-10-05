//! Whether a command runs the project's tests the way the run's suite
//! runs them. A test runner settles what it builds for each invocation,
//! so a command that runs the tests another way — one package, other
//! flags — builds them again beside the suite's build. One that extends
//! the suite's own command reuses that build.

use super::proof::segments;

/// The suite a run measures, as the command a criterion extends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suite {
    /// The suite's first command, word by word, past the variables it
    /// assigns: what a command that runs the tests starts with.
    command: Vec<String>,
    /// How many of those words come before its first option: the
    /// program, and its subcommand, that run the project's tests.
    runner: usize,
}

impl Suite {
    /// The suite `cmd` runs, read from its first command — `None` when
    /// it names no program.
    pub fn of(cmd: &str) -> Option<Suite> {
        let first = segments(cmd).into_iter().next()?;
        let program = first.split('|').next().unwrap_or_default();
        let command: Vec<String> = words(program).map(str::to_string).collect();
        let runner = command
            .iter()
            .position(|word| word.starts_with('-'))
            .unwrap_or(command.len());
        (runner > 0).then_some(Suite { command, runner })
    }

    /// Each command of `cmd` that runs the project's tests without
    /// extending the suite.
    pub fn outside(&self, cmd: &str) -> Vec<String> {
        segments(cmd)
            .into_iter()
            .filter(|segment| {
                let words: Vec<&str> = words(segment).collect();
                let runner = self.command.get(..self.runner).unwrap_or_default();
                begins(&words, runner) && !begins(&words, &self.command)
            })
            .collect()
    }

    /// The suite's command, as a command that extends it starts.
    pub fn command(&self) -> String {
        self.command.join(" ")
    }

    /// What is wrong with `command`, one of what [`Suite::outside`]
    /// found, and how to write it instead.
    pub fn outside_detail(&self, command: &str) -> String {
        format!(
            "`{command}` runs the project's tests outside the suite `{suite}`, so it builds them \
             again beside the suite's build; extend the suite instead — `{suite}` followed by \
             what selects this task's tests",
            suite = self.command()
        )
    }
}

/// The words of a shell command, past the variables it assigns.
fn words(command: &str) -> impl Iterator<Item = &str> {
    command.split_whitespace().skip_while(|word| assigns(word))
}

/// Whether `word` assigns a variable, as `RUST_LOG=debug` does.
fn assigns(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Whether `words` starts with every one of `start`.
fn begins(words: &[&str], start: &[String]) -> bool {
    words.len() >= start.len() && start.iter().zip(words).all(|(want, word)| want == word)
}

#[cfg(test)]
mod tests {
    use super::Suite;

    #[test]
    fn a_command_that_runs_the_tests_another_way_is_outside() {
        let suite = Suite::of("cargo test --workspace").unwrap();
        assert_eq!(
            suite.outside("cargo test -p yunta-engine --test retry"),
            ["cargo test -p yunta-engine --test retry"]
        );
        assert_eq!(suite.outside("cargo build && cargo test"), ["cargo test"]);
        assert!(suite
            .outside("cargo test --workspace --test retry")
            .is_empty());
        assert!(suite
            .outside("RUST_LOG=debug cargo test --workspace --test a")
            .is_empty());
        assert!(suite.outside("cargo clippy -p yunta-engine").is_empty());
        assert!(suite.outside("sh check.sh").is_empty());
    }

    #[test]
    fn the_runner_is_what_comes_before_the_first_option() {
        let make = Suite::of("make test").unwrap();
        assert!(make.outside("make lint").is_empty());
        assert_eq!(make.command(), "make test");
        let pytest = Suite::of("CI=1 pytest -q && echo done").unwrap();
        assert_eq!(pytest.command(), "pytest -q");
        assert_eq!(pytest.outside("pytest tests/a.py"), ["pytest tests/a.py"]);
        assert!(pytest.outside("pytest -q tests/a.py").is_empty());
        assert_eq!(Suite::of("  "), None);
    }
}
