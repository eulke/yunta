//! What a new binary's first run costs on this machine.
//!
//! macOS assesses every executable it has not seen before its first run,
//! and a test suite links a binary per test target: a run that rebuilds
//! them pays the assessment once per binary, every time, outside any test.
//! The probe links a binary nobody ran before, runs it twice the way a run
//! starts a command, and finds what the first run cost over the second, and
//! whether that is enough to slow a suite.

use std::time::{Duration, Instant};

use yunta_core::Env;
use yunta_render::page::doctor::Finding;

/// How much more than its second run a new binary's first run may cost
/// before a suite of them slows a run: a tenth of a second, which two
/// hundred test binaries turn into twenty.
const NOTICEABLE: Duration = Duration::from_millis(100);

/// What the probe found, or `None` where it does not apply — another
/// system, or no C compiler on `env`'s `PATH` to link a probe with.
pub(super) async fn first_run(env: &Env) -> Option<Finding> {
    if std::env::consts::OS != "macos" {
        return None;
    }
    let compiler = env.find("cc")?;
    let (first, then) = measured(&compiler).await?;
    Some(Finding::FirstRun {
        first,
        then,
        slows_a_suite: slows_a_suite(first, then),
    })
}

/// What the first and the second run of a binary `compiler` freshly linked
/// took.
async fn measured(compiler: &std::path::Path) -> Option<(Duration, Duration)> {
    let dir = tempfile::tempdir().ok()?;
    let source = dir.path().join("probe.c");
    // A value only this probe holds, so the binary is one the system has
    // never seen, whatever ran before.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos())
        ^ u128::from(std::process::id());
    let program = format!("int main(void) {{ return {nonce} == 0; }}\n");
    tokio::fs::write(&source, program).await.ok()?;
    let binary = dir.path().join("probe");
    let linked = tokio::process::Command::new(compiler)
        .arg("-o")
        .arg(&binary)
        .arg(&source)
        .output()
        .await
        .ok()?;
    if !linked.status.success() {
        return None;
    }
    let first = timed(&binary).await?;
    let then = timed(&binary).await?;
    Some((first, then))
}

/// How long one run of `binary` took.
async fn timed(binary: &std::path::Path) -> Option<Duration> {
    let started = Instant::now();
    tokio::process::Command::new(binary).status().await.ok()?;
    Some(started.elapsed())
}

/// Whether a first run that took `first`, against a second that took
/// `then`, costs enough over it to slow a suite of binaries.
fn slows_a_suite(first: Duration, then: Duration) -> bool {
    first.saturating_sub(then) >= NOTICEABLE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compiler is the workspace's: one its `PATH` does not have is no
    /// compiler, whatever the process doctor runs in can find.
    #[tokio::test]
    async fn a_workspace_whose_path_has_no_compiler_leaves_the_probe_out() {
        let env = Env {
            path: Vec::new(),
            ..Env::default()
        };
        assert_eq!(first_run(&env).await, None);
    }

    #[test]
    fn a_first_run_that_costs_little_more_than_its_second_slows_nothing() {
        assert!(!slows_a_suite(
            Duration::from_millis(30),
            Duration::from_millis(8)
        ));
    }

    #[test]
    fn a_first_run_a_tenth_of_a_second_over_its_second_slows_a_suite() {
        assert!(slows_a_suite(
            Duration::from_millis(450),
            Duration::from_millis(7)
        ));
        assert!(slows_a_suite(
            Duration::from_millis(108),
            Duration::from_millis(8)
        ));
    }
}
