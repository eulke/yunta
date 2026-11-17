//! `yunta status <run> --node <id>`: one node whole, read from the log
//! of a run whose command failed the way a compiler does, and from the
//! design's run where its loop works its task.

use yunta_testkit::{run_id_from, stderr, stdout, yunta_at, yunta_in, Checkout};
use yunta_testkit_ui::cut::Cut;

/// A build that fails with what went wrong on stdout, in a run that
/// closes on the failure instead of asking what to do about it.
fn failed_build() -> (Checkout, String) {
    let checkout = Checkout::new()
        .config("defaults:\n  isolation: none\n  on_failure: abort\n")
        .workflow(
            "wf",
            "name: compiler\nnodes:\n  - id: build\n    kind: bash\n    run: |\n      \
             printf 'error[E0425]: cannot find value `x` in this scope\\n --> src/lib.rs:3:5\\n'\n      \
             exit 101\n",
        )
        .committed();
    let run = yunta_at!(&checkout, &["run", "wf.yaml"]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let run_id = run_id_from(&run);
    (checkout, run_id)
}

#[test]
fn status_node_prints_the_tail_and_where_the_whole_output_is() {
    let (checkout, run_id) = failed_build();
    let page = yunta_at!(&checkout, &["status", &run_id, "--node", "build"]);
    assert!(page.status.success(), "{}", stderr(&page));
    let text = stdout(&page);
    assert!(
        text.lines()
            .next()
            .is_some_and(|line| line.contains("node build of run") && line.ends_with("failed")),
        "{text}"
    );
    for printed in [
        "error[E0425]: cannot find value `x` in this scope",
        " --> src/lib.rs:3:5",
    ] {
        assert!(text.contains(printed), "`{printed}` in: {text}");
    }
    let whole = text
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("whole output: "))
        .unwrap_or_else(|| panic!("where the whole output is: {text}"));
    let kept = std::fs::read_to_string(whole.replace('~', &checkout.home.display().to_string()))
        .or_else(|_| std::fs::read_to_string(checkout.repo.join(whole)))
        .or_else(|_| std::fs::read_to_string(whole))
        .unwrap_or_else(|e| panic!("`{whole}` names the kept output: {e}"));
    assert!(kept.contains("cannot find value `x`"), "{kept}");
}

#[test]
fn an_unknown_node_suggests_the_closest_id() {
    let (checkout, run_id) = failed_build();
    let page = yunta_at!(&checkout, &["status", &run_id, "--node", "biuld"]);
    assert!(!page.status.success());
    assert!(
        stderr(&page).contains("has no node `biuld`; did you mean `build`?"),
        "{}",
        stderr(&page)
    );
}

/// The loop of the design's run, at work on its task where the target `board` stands, says
/// in print what its pane shows: the task's own row, how it is proved, its scope with the
/// paths its session wrote, and what it is doing, its newest call last.
#[tokio::test]
async fn a_loop_at_work_prints_its_task_s_proof_and_scope_and_its_feed_s_last_rows() {
    let cut = Cut::of_target("board").await;
    let run = cut.runs()[0].to_string();
    let page = yunta_in!(
        &cut.repository(),
        cut.root(),
        &["status", &run, "--node", "implement"]
    );
    assert!(page.status.success(), "{}", stderr(&page));
    let text = stdout(&page);
    let lines: Vec<&str> = text.lines().collect();
    for printed in [
        "● global-pack-install  Add explicit global pack install target · attempt 1",
        "  proved   red",
        "  keeps   lint, green at the baseline",
        "  its scope",
        "  ✓ crates/cli/src/pack.rs",
        "  ◌ crates/cli/tests/pack_cmd.rs",
        "what it is doing · 8 calls",
    ] {
        assert!(lines.contains(&printed), "`{printed}` in:\n{text}");
    }
    assert!(
        lines.iter().any(
            |line| line.starts_with("  by      cargo test --workspace --test pack_global_spec")
        ),
        "{text}"
    );
    let feed = lines
        .iter()
        .position(|line| *line == "what it is doing · 8 calls")
        .expect("the feed's heading");
    let newest = lines[feed + 1..]
        .iter()
        .take_while(|line| !line.is_empty())
        .last();
    assert_eq!(
        newest,
        Some(&"edit    crates/cli/src/cli/dispatch.rs"),
        "{text}"
    );
}
