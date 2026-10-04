//! Running the compiled `yunta` binary and reading its output.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::CliChild;

/// What a terminal a test asks for calls itself.
pub(crate) const TERM: &str = "xterm-256color";

/// Puts `cmd` in the world a test means to measure, rather than
/// whichever one the suite happens to run on.
///
/// The binary reads an org config from a fixed system path unless it is
/// pointed elsewhere, so a machine that has one would decide what a
/// test sees: the org layer is a ceiling the lower layers can only
/// narrow, so one on the host silently changes what every run may do.
/// `home` gets an empty one instead, and `HOME` points there too, so
/// nothing reaches the developer's own `~/.yunta`. `USER` names the
/// author of what the run commits, and the terminal variables decide
/// what it may draw and how wide — all inherited would make the same
/// suite measure differently on two machines.
///
/// Git is pinned the same way and for the same reason: a run commits,
/// and a developer's global or system git config decides the branch a
/// fresh repository starts on, who authors a commit, and whether a hook
/// fires. Both are pointed at files under `home`, which are empty.
pub fn hermetic<C: Spawning>(cmd: &mut C, dir: &Path, home: &Path) {
    std::fs::create_dir_all(home).expect("the test's own home");
    let org_config = home.join("org.yaml");
    std::fs::write(&org_config, "").expect("an empty org config under the test's home");
    let git_config = home.join("gitconfig");
    std::fs::write(&git_config, "").expect("an empty git config under the test's home");
    cmd.runs_in(dir);
    for (name, value) in [
        ("YUNTA_HOME", home.as_os_str()),
        ("YUNTA_ORG_CONFIG", org_config.as_os_str()),
        ("HOME", home.as_os_str()),
        ("USER", OsStr::new("yunta-test")),
        ("TERM", OsStr::new(TERM)),
        // The glyph set, named rather than inherited from whatever locale
        // the machine running the suite has, so a test reads the same
        // characters everywhere.
        ("YUNTA_GLYPHS", OsStr::new("unicode")),
        ("GIT_CONFIG_GLOBAL", git_config.as_os_str()),
        ("GIT_CONFIG_SYSTEM", git_config.as_os_str()),
    ] {
        cmd.carries(name, value);
    }
    for name in [
        "NO_COLOR",
        "CLICOLOR",
        "CLICOLOR_FORCE",
        "COLUMNS",
        "YUNTA_HYPERLINKS",
        "TERM_PROGRAM",
        "VTE_VERSION",
        "KITTY_WINDOW_ID",
        "WT_SESSION",
    ] {
        cmd.drops(name);
    }
}

/// What [`hermetic`] needs of a command, so a test that spawns the
/// binary through tokio is pinned exactly like one that spawns it
/// through the standard library — two ways to start the same process
/// cannot mean two environments.
pub trait Spawning {
    fn runs_in(&mut self, dir: &Path);
    fn carries(&mut self, name: &str, value: &OsStr);
    fn drops(&mut self, name: &str);
}

impl Spawning for Command {
    fn runs_in(&mut self, dir: &Path) {
        self.current_dir(dir);
    }

    fn carries(&mut self, name: &str, value: &OsStr) {
        self.env(name, value);
    }

    fn drops(&mut self, name: &str) {
        self.env_remove(name);
    }
}

impl Spawning for tokio::process::Command {
    fn runs_in(&mut self, dir: &Path) {
        self.current_dir(dir);
    }

    fn carries(&mut self, name: &str, value: &OsStr) {
        self.env(name, value);
    }

    fn drops(&mut self, name: &str) {
        self.env_remove(name);
    }
}

/// Runs the compiled `yunta` binary at `bin` in `dir` with `YUNTA_HOME`
/// pointed at `home` and stdin closed, returning its captured output. Use
/// the [`yunta_in!`](crate::yunta_in) macro rather than calling this
/// directly — it fills in the binary path from the calling crate's
/// `CARGO_BIN_EXE_yunta`.
pub fn run_yunta(bin: &Path, dir: &Path, home: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(bin);
    hermetic(&mut cmd, dir, home);
    cmd.args(args).stdin(Stdio::null());
    CliChild::spawn(cmd, None)
        .expect("failed to spawn the yunta binary")
        .wait_with_output()
        .expect("the yunta binary did not finish within the test deadline")
}

/// The command's stdout as an owned `String` (lossy on non-UTF-8).
pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The command's stderr as an owned `String` (lossy on non-UTF-8).
pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The checkouts of every project's pool under the state root `home`: the
/// checkouts runs worked in, wherever the pool put them.
pub fn pool_checkouts(home: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(pools) = std::fs::read_dir(home.join("worktrees").join("pool")) else {
        return Vec::new();
    };
    let mut checkouts: Vec<std::path::PathBuf> = pools
        .flatten()
        .filter_map(|pool| std::fs::read_dir(pool.path()).ok())
        .flat_map(|slots| slots.flatten().map(|slot| slot.path()))
        .filter(|slot| slot.is_dir())
        .collect();
    checkouts.sort();
    checkouts
}

/// The run id `yunta run` prints, parsed from a `run <id>: …` line or the
/// id alone `--quiet` prints — the
/// handle every follow-up command (`status`, `receipt`, `graph --run`)
/// needs. Panics if no such line is present, naming what it saw.
pub fn run_id_from(output: &Output) -> String {
    find_run_id(&stdout(output))
        .or_else(|| find_run_id(&stderr(output)))
        .unwrap_or_else(|| {
            panic!(
                "no `run <id>:` line in output:\nstdout:\n{}\nstderr:\n{}",
                stdout(output),
                stderr(output)
            )
        })
}

/// The same, from output a test collected some other way — a log file
/// the command was spawned onto.
pub fn run_id_in(text: &str) -> String {
    find_run_id(text).unwrap_or_else(|| panic!("no `run <id>:` line in output:\n{text}"))
}

/// What every line a person reads calls the run whose id is `id`.
pub fn handle(id: &str) -> &str {
    let start = id.len().saturating_sub(yunta_core::RunId::HANDLE_CHARS);
    id.get(start..).unwrap_or(id)
}

/// The whole id of the run `called` names under `home`: `called` is the
/// handle a line a person reads prints — what [`run_id_from`] finds — or
/// the id itself. For a test that opens the run's own directory, which
/// is named by the whole id.
pub fn full_run_id(home: &Path, called: &str) -> String {
    let runs = std::fs::read_dir(crate::runs_root(home)).expect("the runs under the test's home");
    let named: Vec<String> = runs
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|id| id.ends_with(called))
        .collect();
    match named.as_slice() {
        [one] => one.clone(),
        other => panic!("`{called}` names {} runs: {other:?}", other.len()),
    }
}

fn find_run_id(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        // `--quiet` prints the id alone: a ULID, 26 characters of
        // Crockford base32.
        let alone = line.trim();
        if alone.len() == 26
            && alone
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
        {
            return Some(alone.to_string());
        }
        line.strip_prefix("run ")
            .and_then(|rest| rest.split(':').next())
            .map(str::to_string)
    })
}

/// What the first checklist row about `subject` in `text` says; `None`
/// when no row checks `subject`. See [`checks`].
pub fn checked(text: &str, subject: &str) -> Option<String> {
    checks(text, subject).into_iter().next()
}

/// What every checklist row about `subject` in `text` says, in order: a
/// row whose mark is followed by `subject`, and the lines it wraps onto,
/// joined with single spaces.
pub fn checks(text: &str, subject: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (at, row) in lines.iter().enumerate() {
        let trimmed = row.trim_start();
        let Some((mark, rest)) = trimmed.split_once(' ') else {
            continue;
        };
        let Some(first) = rest.strip_prefix(subject) else {
            continue;
        };
        if mark.chars().count() != 1 {
            continue;
        }
        let depth = row.len() - trimmed.len();
        let wrapped = lines
            .iter()
            .skip(at + 1)
            .take_while(|line| {
                !line.trim().is_empty() && line.len() - line.trim_start().len() > depth
            })
            .copied();
        found.push(
            std::iter::once(first)
                .chain(wrapped)
                .flat_map(str::split_whitespace)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    found
}
