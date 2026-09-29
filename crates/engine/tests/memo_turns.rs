//! Two checks of one command on one tree at the same moment — the tasks
//! of a batch, each pre-checking the suite on the commit they all start
//! from — run it once: the second waits for the first's answer.

use yunta_core::events::{CriterionType, TaskLedger};
use yunta_core::{Criterion, Task};
use yunta_engine::Memo;
use yunta_testkit::{init_repo, Owner};

fn guarded(id: &str, cmd: &str) -> Task {
    Task {
        id: id.into(),
        title: id.to_string(),
        scope: vec![format!("{id}.txt").as_str().into()],
        criteria: vec![Criterion {
            cmd: cmd.to_string(),
            r#type: Some(CriterionType::Guard),
            proves: None,
        }],
        depends_on: Vec::new(),
        notes: None,
        description: None,
    }
}

#[tokio::test]
async fn two_pre_checks_of_one_command_on_one_tree_at_once_run_it_once() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    tokio::fs::create_dir_all(&repo).await.unwrap();
    init_repo(&repo);
    // Outside the repo, so counting the runs never changes the tree the
    // cache is keyed on; the pause keeps the first run open while the
    // second check asks.
    let runs = root.path().join("runs.txt");
    let suite = format!("echo ran >> {}; sleep 0.3", runs.display());
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));
    let history = TaskLedger::default();
    let (first, second) = (guarded("T001", &suite), guarded("T002", &suite));

    let (a, b) = tokio::join!(
        yunta_engine::pre_check(&first, &repo, &memo, &history, owner.supervision()),
        yunta_engine::pre_check(&second, &repo, &memo, &history, owner.supervision()),
    );
    let (a, b) = (a.unwrap(), b.unwrap());

    let ran = tokio::fs::read_to_string(&runs).await.unwrap();
    assert_eq!(ran.lines().count(), 1, "the suite ran once");
    assert_eq!(
        [a[0].reused, b[0].reused]
            .iter()
            .filter(|reused| **reused)
            .count(),
        1,
        "and one of the two checks says it reused that answer"
    );
    assert_eq!((a[0].exit_code, b[0].exit_code), (0, 0));
}
