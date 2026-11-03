//! One run says one thing, wherever it is read: its page, the listing, the line `--quiet`
//! leaves and the document a program reads give the same word, the same counts and the same
//! next steps.

use serde_json::Value;
use yunta_testkit::{git, run_id_from, stderr, stdout, yunta_in, Checkout};

const CONFIG: &str = "\
defaults:
  isolation: none
runners:
  planner:
    - { adapter: mock, model: mock-model }
";

const FINISHED: &str = "\
name: quick
nodes:
  - { id: first, kind: bash, run: \"echo one > one.txt\" }
  - { id: second, kind: bash, depends_on: [first], run: \"echo two > two.txt\" }
";

const AT_A_GATE: &str = "\
name: gated
nodes:
  - { id: first, kind: bash, run: \"true\" }
  - { id: ok, kind: gate, depends_on: [first], assignee: lead, message: \"Go on?\", options: [go] }
  - { id: last, kind: bash, depends_on: [ok], run: \"true\" }
";

const FAILING: &str = "\
name: broken
nodes:
  - { id: lint, kind: bash, run: \"echo nope >&2; exit 3\" }
  - { id: after, kind: bash, depends_on: [lint], run: \"true\" }
";

const RUN: [&str; 3] = ["run", "wf.yaml", "--quiet"];

/// A run of `workflow`, off a terminal: its id, the line `--quiet` left, and its exit code.
fn ran(checkout: &Checkout) -> (String, String, Option<i32>) {
    let run = yunta_in!(&checkout.repo, &checkout.home, &RUN);
    (run_id_from(&run), stderr(&run), run.status.code())
}

fn checkout(root: &std::path::Path, workflow: &str) -> Checkout {
    Checkout::under(root)
        .config(CONFIG)
        .workflow("wf", workflow)
        .committed()
}

/// Every run of whitespace a single space: a page wraps what a document says in one line.
fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn status_json(checkout: &Checkout, run: &str) -> Value {
    let status = yunta_in!(&checkout.repo, &checkout.home, &["status", run, "--json"]);
    serde_json::from_slice(&status.stdout).unwrap_or_else(|e| panic!("{e}\n{}", stdout(&status)))
}

fn status_page(checkout: &Checkout, run: &str) -> String {
    let status = yunta_in!(&checkout.repo, &checkout.home, &["status", run]);
    folded(&stdout(&status))
}

fn listed(checkout: &Checkout, run: &str) -> Value {
    let list = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["list", "--runs", "--json"]
    );
    let listing: Value =
        serde_json::from_slice(&list.stdout).unwrap_or_else(|e| panic!("{e}\n{}", stdout(&list)));
    listing["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["run_id"] == run)
        .cloned()
        .expect("the run is listed")
}

/// Each next step a document names, in the words the page says it, by the run's handle where
/// the page reads a run by it.
fn next_steps_stand_on_the_page(document: &Value, page: &str, run: &str, handle: &str) {
    let next = document["next"].as_array().expect("the next steps");
    assert!(!next.is_empty(), "{document:#}");
    for step in next {
        let command = step["command"].as_str().expect("a command");
        assert!(
            command.contains(run),
            "the whole id, for a program: {command}"
        );
        let spoken = folded(&command.replace(run, handle));
        assert!(
            page.contains(&spoken),
            "`{spoken}` is not on the page: {page}"
        );
        let why = folded(step["why"].as_str().expect("what it does"));
        assert!(page.contains(&why), "`{why}` is not on the page: {page}");
    }
}

/// What every surface of a run has to agree on.
fn says_one_thing(checkout: &Checkout) -> Value {
    let (run, quiet, code) = ran(checkout);
    let handle = yunta_testkit::handle(&run).to_string();
    let document = status_json(checkout, &run);
    let page = status_page(checkout, &run);
    let row = listed(checkout, &run);
    let word = document["outcome"].as_str().expect("the run's word");

    assert_eq!(row["word"], word, "the listing: {row:#}");
    assert!(page.contains(word), "the page: {page}");
    assert!(quiet.contains(word), "the line --quiet leaves: {quiet}");
    assert_eq!(
        document["exit_code"].as_i64().map(|code| code as i32),
        code,
        "{document:#}"
    );
    assert_eq!(document["handle"], handle.as_str());

    let nodes = &document["counts"]["nodes"];
    let finished = nodes["by"]["finished"].as_u64().unwrap_or(0);
    let total = nodes["total"].as_u64().expect("how many nodes");
    assert!(
        page.contains(&format!("{finished} of {total} node")),
        "the counts of {document:#} are not what the page says: {page}"
    );

    next_steps_stand_on_the_page(&document, &page, &run, &handle);

    if let Some(said) = document["said"].as_str() {
        assert!(
            page.contains(&folded(said)),
            "`{said}` is not on the page: {page}"
        );
    }
    assert_eq!(
        row["said"].as_str().map(str::is_empty),
        Some(word == "finished"),
        "a finished run's row has nothing to add: {row:#}"
    );
    assert_eq!(
        row["furrow"].as_array().map(Vec::len),
        Some(total as usize),
        "a word for every node: {row:#}"
    );
    document
}

#[test]
fn a_finished_run_says_one_thing_and_what_it_came_to() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), FINISHED);
    let document = says_one_thing(&checkout);
    assert_eq!(document["outcome"], "finished");
    assert_eq!(document["workflow"], "quick");
    assert_eq!(document["mode"], "default");
    assert_eq!(document["counts"]["nodes"]["total"], 2);
    assert_eq!(document["counts"]["nodes"]["by"]["finished"], 2);
    assert!(document["elapsed_secs"].as_f64().is_some(), "{document:#}");
    assert!(
        document["time"]["people_secs"].as_f64().is_some(),
        "{document:#}"
    );
    assert!(document["branch"]["base"].is_string(), "{document:#}");
    assert!(document["branch"].get("name").is_none(), "{document:#}");
    assert!(document.get("pull_requests").is_none(), "{document:#}");
}

#[test]
fn a_run_at_a_gate_says_one_thing_and_what_it_asks() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), AT_A_GATE);
    let document = says_one_thing(&checkout);
    assert_eq!(document["outcome"], "needs you");
    assert_eq!(document["exit_code"], 3);
    assert_eq!(document["counts"]["nodes"]["by"]["finished"], 1);
    assert_eq!(document["said"], "Go on?", "{document:#}");
}

#[test]
fn a_failed_node_says_one_thing_and_why() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), FAILING);
    let document = says_one_thing(&checkout);
    assert_eq!(document["outcome"], "needs you");
    assert_eq!(document["counts"]["nodes"]["by"]["failed"], 1);
    assert_eq!(document["counts"]["nodes"]["by"]["never ran"], 1);
}

/// A key said twice is a document half of its readers refuse, and the other half read
/// whichever came last.
#[test]
fn waiting_on_says_what_it_waits_on_once() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), AT_A_GATE);
    let (run, _, _) = ran(&checkout);
    let status = yunta_in!(&checkout.repo, &checkout.home, &["status", &run, "--json"]);
    let raw = stdout(&status);
    let at = raw.find("\"waiting_on\"").expect("what it waits on");
    let block = &raw[at..];
    let end = block.find('}').expect("its end");
    assert_eq!(
        block[..end].matches("\"on\"").count(),
        1,
        "{}",
        &block[..end]
    );
    let document: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(document["waiting_on"]["on"], "gate", "{document:#}");
    assert_eq!(document["waiting_on"]["node"], "ok", "{document:#}");
}

/// The run's changes are the same fact the page's `changed` row and the receipt say.
#[test]
fn what_a_run_changed_is_in_its_document() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config("defaults:\n  isolation: worktree\n")
        .workflow("wf", FINISHED)
        .committed();
    let (run, _, _) = ran(&checkout);
    let document = status_json(&checkout, &run);
    assert_eq!(document["changed"]["files"], 2, "{document:#}");
    assert_eq!(document["changed"]["added"], 2, "{document:#}");
    let name = document["branch"]["name"].as_str().expect("its branch");
    assert!(name.contains(&run), "{name}");
}

#[test]
fn verify_says_its_word_for_a_program_too() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), FINISHED);
    let (run, _, _) = ran(&checkout);
    let verify = yunta_in!(&checkout.repo, &checkout.home, &["verify", &run, "--json"]);
    let document: Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(document["word"], "intact", "{document:#}");
    assert_eq!(document["holds"], true, "{document:#}");

    let doctor = yunta_in!(&checkout.repo, &checkout.home, &["doctor", "--json"]);
    let document: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let word = document["word"].as_str().expect("its word");
    assert!(word == "ready" || word == "not ready", "{document:#}");
}

/// Four nodes in a chain. Under a cap of 100 each session may spend an equal share of what is
/// left across the nodes still owed: the first three spend 25, 33 and 42, the whole cap, and the
/// run parks before the fourth, whose 300 is what makes the cap read low against its history.
const SPENDING: &str = "\
name: spending
nodes:
  - { id: first, kind: prompt, runner: planner, prompt: \"One.\" }
  - { id: second, kind: prompt, runner: planner, depends_on: [first], prompt: \"Two.\" }
  - { id: third, kind: prompt, runner: planner, depends_on: [second], prompt: \"Three.\" }
  - { id: more, kind: prompt, runner: planner, depends_on: [third], prompt: \"Four.\" }
";

const SPENDS: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 25, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 33, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 42, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 300, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
"#;

/// A run parked on its budget says what the cap means against its history, whichever
/// invocation is asked: one run, one document.
#[test]
fn a_run_parked_on_its_budget_says_the_same_caution_when_resumed() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config(CONFIG)
        .workflow("wf", SPENDING)
        .file("fixture.yaml", SPENDS)
        .committed();
    let mocked = [
        "run",
        "wf.yaml",
        "--adapter",
        "mock",
        "--fixture",
        "fixture.yaml",
        "--quiet",
    ];
    // Three runs are what earn a workflow an estimate at all.
    for _ in 0..3 {
        let run = yunta_in!(&checkout.repo, &checkout.home, &mocked);
        assert!(run.status.success(), "{}", stderr(&run));
    }
    std::fs::write(
        checkout.repo.join(".yunta/config.yaml"),
        format!("{CONFIG}limits:\n  max_tokens_per_run: 100\n"),
    )
    .unwrap();
    git(&checkout.repo, &["add", "."]);
    git(&checkout.repo, &["commit", "-q", "-m", "a lower budget"]);
    let run = yunta_in!(&checkout.repo, &checkout.home, &mocked);
    let id = run_id_from(&run);

    let status = status_json(&checkout, &id);
    assert!(
        status["cautions"]
            .as_array()
            .is_some_and(|said| !said.is_empty()),
        "{status:#}"
    );
    assert!(status["budget_warning"].is_string(), "{status:#}");

    let resumed = yunta_in!(&checkout.repo, &checkout.home, &["resume", &id, "--json"]);
    let resumed: Value = serde_json::from_slice(&resumed.stdout)
        .unwrap_or_else(|e| panic!("{e}\n{}\n{}", stdout(&resumed), stderr(&resumed)));
    assert_eq!(resumed["cautions"], status["cautions"], "{resumed:#}");
    assert_eq!(
        resumed["budget_warning"], status["budget_warning"],
        "{resumed:#}"
    );
}
