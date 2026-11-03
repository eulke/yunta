//! What a tool call does, read from what a CLI reports, the same for every CLI.
//!
//! A call's argument is the session's own text, which may be anything at all,
//! a secret among it. So a reader is told only what the engine can vouch for: a
//! path inside the session's checkout, or a program and its subcommand, copied
//! out of the tables here and never out of the text. A pattern, a URL or an
//! argument is never shown. A command's digest is of its whole text, and a
//! path's is of the path as shown, so two spellings of a file read as one target.
//!
//! This is a reading, for a person, and it stays one: what a command does when it
//! runs is the command's. A `cd` in an earlier call is not followed.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use yunta_core::events::{ToolAction, ToolTarget};
use yunta_core::shell;

/// What moves a command's directory: a word after it is read against a directory
/// the reading no longer knows, so it vouches for no file.
const MOVES: &[&str] = &["cd", "pushd", "popd"];
/// What only sets a command up: a reader is told what comes after it.
const PREPARES: &[&str] = &["export", "set", "unset", "source", ".", ":", "true"];
/// What only prints: a reader is told what comes after it, unless it prints into
/// a file.
const PRINTS: &[&str] = &["echo", "printf"];
/// The programs that run the script they are handed.
const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash"];
/// The programs that only read the files they are given, unless told to change
/// them in place, or to print into a file.
const READS: &[&str] = &[
    "cat", "head", "tail", "nl", "less", "more", "bat", "wc", "awk", "sed",
];
/// The flags that have a search change what it finds.
const SEARCH_ACTS: &[&str] = &[
    "-delete",
    "-exec",
    "-execdir",
    "-ok",
    "-okdir",
    "-x",
    "--exec",
    "-X",
    "--exec-batch",
];
/// How a fetch is told to send something, each flag as it starts.
const FETCH_SENDS: &[&str] = &[
    "-X",
    "-d",
    "--data",
    "-F",
    "--form",
    "-T",
    "--upload-file",
    "--post-data",
    "--post-file",
];
/// The programs that change the files they are given.
const EDITS: &[&str] = &["apply_patch", "patch"];
/// The programs that look for something: what they look for is the session's own.
const SEARCHES: &[&str] = &["rg", "grep", "egrep", "fgrep", "ag", "ack", "fd", "find"];
/// The programs that list what is there.
const LISTS: &[&str] = &["ls", "tree", "eza", "exa"];
/// The programs that fetch something: where from is the session's own.
const FETCHES: &[&str] = &["curl", "wget"];
/// The programs whose subcommand says what they do, each with the ones a
/// reader may be told.
const SUBCOMMANDS: &[(&str, &[&str])] = &[
    (
        "cargo",
        &[
            "bench", "build", "check", "clippy", "doc", "fmt", "nextest", "run", "test", "xtask",
        ],
    ),
    (
        "git",
        &[
            "add",
            "apply",
            "blame",
            "branch",
            "checkout",
            "cherry-pick",
            "commit",
            "diff",
            "fetch",
            "log",
            "merge",
            "pull",
            "push",
            "rebase",
            "reset",
            "restore",
            "rev-parse",
            "rm",
            "show",
            "stash",
            "status",
            "switch",
            "tag",
            "worktree",
        ],
    ),
    ("npm", &["ci", "install", "run", "test"]),
    ("pnpm", &["install", "run", "test"]),
    ("yarn", &["install", "run", "test"]),
    (
        "go",
        &["build", "fmt", "generate", "mod", "run", "test", "vet"],
    ),
    ("uv", &["add", "pip", "run", "sync"]),
    ("pip", &["install", "list", "show"]),
    ("docker", &["build", "compose", "ps", "run"]),
    ("gh", &["issue", "pr", "release", "run"]),
];
/// The programs a reader may be told by name alone: what comes after them is
/// the repository's or the session's own.
const PROGRAMS: &[&str] = &[
    "make", "just", "pytest", "tsc", "eslint", "ruff", "mypy", "rustc", "node", "python",
    "python3", "mv", "cp", "rm", "mkdir", "touch", "chmod",
];
/// How many shells within shells are opened to read what they run. A real
/// command has one: past this, nothing more is worth the reading.
const WRAPPERS: usize = 3;

/// The tree a session works in, by the two names a path may give it: as it was
/// asked for, and as the file system resolves it.
#[derive(Debug, Clone)]
pub(crate) struct Checkout {
    root: PathBuf,
    resolved: Option<PathBuf>,
}

impl Checkout {
    /// The checkout at `root`.
    pub(crate) fn at(root: &Path) -> Self {
        Checkout {
            root: root.to_path_buf(),
            resolved: std::fs::canonicalize(root).ok(),
        }
    }

    /// `path` relative to the checkout, when the engine can tell it stays inside
    /// it: a `..` is not followed, since where it leads takes the file system to
    /// say, and a `~` or a variable is the shell's to expand.
    fn inside(&self, path: &str) -> Option<PathBuf> {
        if path.starts_with('~') || path.contains(['$', '`']) {
            return None;
        }
        let path = Path::new(path);
        let relative = match path.is_absolute() {
            true => [Some(&self.root), self.resolved.as_ref()]
                .into_iter()
                .flatten()
                .find_map(|root| path.strip_prefix(root).ok())?,
            false => path,
        };
        let mut inside = PathBuf::new();
        for part in relative.components() {
            match part {
                Component::Normal(name) => inside.push(name),
                Component::CurDir => {}
                _ => return None,
            }
        }
        (!inside.as_os_str().is_empty()).then_some(inside)
    }

    /// `word` of a command, when it names a file the checkout holds: a word is
    /// not known to be a path until the file is there.
    fn holds_file(&self, word: &str) -> Option<PathBuf> {
        let inside = self.inside(word)?;
        self.root.join(&inside).is_file().then_some(inside)
    }
}

/// A call whose argument is a path: its action, and the path where it stays
/// inside the checkout, relative to it.
pub(crate) fn of_path(
    action: ToolAction,
    path: &str,
    checkout: &Checkout,
) -> (ToolAction, ToolTarget) {
    let target = match checkout.inside(path) {
        Some(inside) => ToolTarget::of_path(&inside),
        None => ToolTarget::opaque(path.as_bytes()),
    };
    (action, target)
}

/// A call on a tool of another server: named by the server and the tool, names
/// the workflow itself declares, so a reader may see them. The same under every CLI.
pub(crate) fn of_other_server(server: &str, tool: &str) -> (ToolAction, ToolTarget) {
    (
        ToolAction::Call,
        ToolTarget::named(&format!("{server}:{tool}")),
    )
}

/// A command line, read by the first command in it that does something: its
/// wrapper opened, and what only prepares skipped. A pipeline is read by its
/// first command, and a line with nothing to read is a run of nothing shown.
pub(crate) fn of_command(command: &str, checkout: &Checkout) -> (ToolAction, ToolTarget) {
    let start = Reading {
        checkout,
        wrappers: WRAPPERS,
        moved: false,
    };
    let (action, shown) = start.act(command).unwrap_or((ToolAction::Run, None));
    let target = match shown {
        Some(shown) => ToolTarget::showing(&shown, command.as_bytes()),
        None => ToolTarget::opaque(command.as_bytes()),
    };
    (action, target)
}

/// What a command line comes to: an action, and what a reader may be told of it.
type Act = (ToolAction, Option<String>);

/// Where a reading of a command line stands: how many more shells it opens, and
/// whether it has left the directory it knows.
#[derive(Clone, Copy)]
struct Reading<'a> {
    checkout: &'a Checkout,
    wrappers: usize,
    moved: bool,
}

impl Reading<'_> {
    /// What `script` comes to: the first of its commands that does something.
    fn act(mut self, script: &str) -> Option<Act> {
        shell::simple_commands(script).iter().find_map(|command| {
            let command = without_heredoc(command);
            let words = shell::words(command);
            let mut words = words
                .iter()
                .map(String::as_str)
                .skip_while(|word| is_assignment(word) || matches!(*word, "(" | "{"));
            let program = program_of(words.next()?.trim_start_matches('('));
            let rest: Vec<&str> = words.collect();
            if MOVES.contains(&program) {
                self.moved = true;
                return None;
            }
            if PREPARES.contains(&program) {
                return None;
            }
            self.command(program, &rest, command)
        })
    }

    /// What one command comes to: the file it prints into, shown where it stays
    /// inside the checkout, the shell it opens, or what its program does. `None`
    /// for one that only prints, to no file.
    fn command(self, program: &str, rest: &[&str], whole: &str) -> Option<Act> {
        if READS.contains(&program) || PRINTS.contains(&program) {
            if let Some(file) = output_file(whole) {
                let inside = self.checkout.inside(&file);
                let shown = inside.and_then(|path| self.vouched(path.display().to_string()));
                return Some((ToolAction::Write, shown));
            }
        }
        if PRINTS.contains(&program) {
            return None;
        }
        match wrapped(program, rest) {
            Some(inner) if self.wrappers > 0 => Reading {
                wrappers: self.wrappers - 1,
                ..self
            }
            .act(inner),
            _ => Some(self.named(program, rest)),
        }
    }

    /// `shown`, unless the reading has left the directory it was read against.
    fn vouched(self, shown: String) -> Option<String> {
        (!self.moved).then_some(shown)
    }

    /// What `program` does, by the tables.
    fn named(self, program: &str, rest: &[&str]) -> Act {
        let file = || {
            rest.iter()
                .filter(|word| !word.starts_with('-'))
                .find_map(|word| self.checkout.holds_file(word))
                .and_then(|path| self.vouched(path.display().to_string()))
        };
        let has_flag = |flags: &[&str]| rest.iter().any(|word| flags.contains(word));
        let starts_a_flag = |flags: &[&str]| {
            rest.iter()
                .any(|word| flags.iter().any(|flag| word.starts_with(flag)))
        };
        match program {
            "sed" | "awk" if edits_in_place(program, rest) => (ToolAction::Edit, file()),
            "git" if rest.first() == Some(&"grep") => (ToolAction::Search, None),
            "find" | "fd" if has_flag(SEARCH_ACTS) => (ToolAction::Run, Some(program.to_string())),
            "curl" | "wget" if starts_a_flag(FETCH_SENDS) => {
                (ToolAction::Run, Some(program.to_string()))
            }
            _ if READS.contains(&program) => (ToolAction::Read, file()),
            _ if EDITS.contains(&program) => (ToolAction::Edit, file()),
            _ if SEARCHES.contains(&program) => (ToolAction::Search, None),
            _ if LISTS.contains(&program) => (ToolAction::List, None),
            _ if FETCHES.contains(&program) => (ToolAction::Fetch, None),
            _ => (ToolAction::Run, run_of(program, rest)),
        }
    }
}

/// A command without the body of a heredoc it opens: that is text it is handed,
/// and none of its words.
fn without_heredoc(command: &str) -> &str {
    match command.contains("<<") {
        true => command.lines().next().unwrap_or(command),
        false => command,
    }
}

/// The file a command's output goes to, when it goes to one: a device is no file
/// a session wrote.
fn output_file(command: &str) -> Option<String> {
    shell::output_files(command)
        .into_iter()
        .find(|file| !file.starts_with("/dev/"))
}

/// Whether `program`'s flags have it change its file in place: for awk `-i inplace`,
/// since its other flags take values; for sed `--in-place`, or `-i` by itself, with
/// a suffix, or after flags that take no value.
fn edits_in_place(program: &str, rest: &[&str]) -> bool {
    if program == "awk" {
        return rest.windows(2).any(|pair| pair == ["-i", "inplace"]);
    }
    rest.iter().any(|word| {
        word.starts_with("--in-place")
            || word
                .strip_prefix('-')
                .and_then(|flags| flags.split_once('i'))
                .is_some_and(|(before, _)| before.chars().all(|flag| "nrEsuz".contains(flag)))
    })
}

/// Whether `word` sets a variable for the command after it.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .chars()
                .all(|sign| sign.is_alphanumeric() || sign == '_')
    })
}

/// The program a command's first word runs, wherever it is run from.
fn program_of(word: &str) -> &str {
    Path::new(word)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(word)
}

/// The script a shell is handed with `-c`, `-lc` and the like.
fn wrapped<'a>(program: &str, rest: &[&'a str]) -> Option<&'a str> {
    if !SHELLS.contains(&program) {
        return None;
    }
    let flag = rest
        .iter()
        .position(|word| word.starts_with('-') && !word.starts_with("--") && word.ends_with('c'))?;
    rest.get(flag + 1).copied()
}

/// What a reader may be told a program runs: words of the tables, and none of
/// the command's own. A flag's value is not a subcommand, so a command that
/// leads with one is told by its program alone.
fn run_of(program: &str, rest: &[&str]) -> Option<String> {
    if let Some((name, known)) = SUBCOMMANDS.iter().find(|(name, _)| *name == program) {
        let first = rest.iter().find(|word| !word.starts_with('-'));
        let subcommand = first.and_then(|word| known.iter().find(|sub| *sub == word));
        return Some(match subcommand {
            Some(subcommand) => format!("{name} {subcommand}"),
            None => (*name).to_string(),
        });
    }
    PROGRAMS
        .iter()
        .find(|name| **name == program)
        .map(|name| (*name).to_string())
}
#[cfg(test)]
mod tests;
