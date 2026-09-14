//! Running the compiled `yunta` binary and reading its output.

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
/// `home` gets an empty one instead. `USER` names the author of what the
/// run commits, and the two terminal variables decide what it may draw
/// — all three inherited would make the same suite measure differently
/// on two machines.
pub fn hermetic(cmd: &mut Command, dir: &Path, home: &Path) {
    std::fs::create_dir_all(home).expect("the test's own home");
    let org_config = home.join("org.yaml");
    std::fs::write(&org_config, "").expect("an empty org config under the test's home");
    cmd.current_dir(dir)
        .env("YUNTA_HOME", home)
        .env("YUNTA_ORG_CONFIG", &org_config)
        .env("USER", "yunta-test")
        .env("TERM", TERM)
        .env_remove("NO_COLOR");
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
