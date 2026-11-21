//! `yunta status <run> --node <id>`: one node whole, read from the log
//! of a run whose command failed the way a compiler does, and from the
//! design's run where its loop works its task and where `plan` works,
//! its nodes before and after it saying what their own scene says.

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

/// The design's run where `plan` works: a node that ended prints the fields its own scene
/// says, the word it stands at, what it took and what it spent, its tokens in and out kept
/// after them; and one the run has not come to says so, as the scene does.
#[tokio::test]
async fn a_nodes_page_prints_the_fields_its_own_scene_says() {
    let cut = Cut::of_target("node-ended").await;
    let run = cut.runs()[0].to_string();
    let page = |node: &str| {
        let page = yunta_in!(
            &cut.repository(),
            cut.root(),
            &["status", &run, "--node", node]
        );
        assert!(page.status.success(), "{}", stderr(&page));
        stdout(&page)
    };
    let ended = page("brief");
    for printed in [
        "  stands       finished",
        "  took         22s",
        "  tokens       112k tokens · 106k in / 6k out (102k cached)",
    ] {
        assert!(
            ended.lines().any(|line| line == printed),
            "`{printed}` in:\n{ended}"
        );
    }
    let never_ran = page("spec");
    assert!(
        never_ran
            .lines()
            .any(|line| line == "  It has not run yet."),
        "{never_ran}"
    );
}

/// A node the run has not come to names on its page the runner its pane's title names: the
/// one its run resolved for the role it names.
#[tokio::test]
async fn a_node_not_run_yet_names_on_its_page_the_runner_its_pane_names() {
    let cut = Cut::of_target("node-never-ran").await;
    let run = cut.runs()[0].to_string();
    let page = yunta_in!(
        &cut.repository(),
        cut.root(),
        &["status", &run, "--node", "spec"]
    );
    assert!(page.status.success(), "{}", stderr(&page));
    let text = stdout(&page);
    assert!(
        text.lines()
            .any(|line| line == "  runner       planner on mock/gpt-6-luna"),
        "{text}"
    );
}

/// The gate a person's `adjust` sent the run back from says one standing on its page: its
/// headline, as its `stands` row, reads rerouted, as the nodes beside its pane mark it.
#[tokio::test]
async fn a_gate_that_sent_the_run_back_stands_rerouted_on_its_whole_page() {
    let cut = Cut::of_target("node-sent-back").await;
    let run = cut.runs()[0].to_string();
    let page = yunta_in!(
        &cut.repository(),
        cut.root(),
        &["status", &run, "--node", "approve-plan"]
    );
    assert!(page.status.success(), "{}", stderr(&page));
    let text = stdout(&page);
    let headline = text.lines().next().unwrap_or_default();
    assert!(
        headline.starts_with("↻ node approve-plan of run") && headline.ends_with("· rerouted"),
        "{text}"
    );
    assert!(
        text.lines().any(|line| line == "  stands       rerouted"),
        "{text}"
    );
}
