//! Running the compiled `yunta` binary and reading its output.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output, Stdio};

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
/// author of what the run commits, and the two terminal variables
/// decide what it may draw — all inherited would make the same suite
/// measure differently on two machines.
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
        ("GIT_CONFIG_GLOBAL", git_config.as_os_str()),
        ("GIT_CONFIG_SYSTEM", git_config.as_os_str()),
    ] {
        cmd.carries(name, value);
    }
    cmd.drops("NO_COLOR");
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
    cmd.args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run the yunta binary")
}

/// The command's stdout as an owned `String` (lossy on non-UTF-8).
pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The command's stderr as an owned `String` (lossy on non-UTF-8).
pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The run id `yunta run` prints, parsed from a `run <id>: …` line — the
/// handle every follow-up command (`status`, `receipt`, `graph --run`)
/// needs. Panics if no such line is present, naming what it saw.
pub fn run_id_from(output: &Output) -> String {
    let text = stdout(output);
    text.lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .unwrap_or_else(|| panic!("no `run <id>:` line in output:\n{text}"))
}
