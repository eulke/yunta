//! The documents a decision shows — a plan as the run judges it, a spec —
//! drawn whole on every surface and held to their goldens.

use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, Environment, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Markdown, Surface, Terminal};
use yunta_core::shown::{
    Answered, DeniedChange, HeldTo, JudgedCriterion, PlanReview, ShownContent, ShownDocument,
    TaskReview,
};
use yunta_core::{RunId, SpecFile, TasksFile};
use yunta_testkit_core::spell;

use super::console_page::{assert_fits, unpainted};
use crate::plan::{document, Form};

pub(super) fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/document")
}

/// The run the goldens name: its handle is the one they print.
pub(super) fn run() -> RunId {
    RunId::from_static("7E5PH4")
}

const PLAN: &str = r#"
summary: Greet a person by name, and say goodbye too.
description: |-
  The program says nothing today. It will greet whoever runs it by name, and say goodbye.

  ```mermaid
  graph LR
    Name --> Greeting
  ```

  A goodbye follows the greeting, on its own line.
design: |-
  `Greeter` owns the words; `main` reads the name and prints.
decisions:
  - id: default-name
    question: What does it say when no name is given?
    choice: It greets `world`.
    alternatives: [Refuse to run, Ask for a name]
    why: A program run with no arguments should still say something.
  - id: greeting-word
    question: Which word greets?
    choice: It says `hello`.
    answers: greeting-word
shapes:
  - name: Greeter
    owner: greet
    file: src/greet.rs
    code: |-
      pub struct Greeter {
          pub name: String,
      }

      impl Greeter {
          pub fn hello(&self) -> String;
      }
risks:
  - A name with a newline in it breaks the line it is printed on.
out_of_scope:
  - Greeting in other languages.
tasks:
  - id: greet
    title: Greet by name
    scope: [src/greet.rs, src/main.rs, tests/greet.rs]
    criteria:
      - cmd: cargo test --test greet
        proves: the greeting names the person
    changes:
      - at: src/greet.rs::Greeter
        what: the type that holds the name and words the greeting
      - at: tests/greet.rs
        what: a test that runs the program with a name
    outcome: "`hello ana` prints `hello, ana`."
    invariants:
      - With no name it still greets `world`.
  - id: farewell
    title: Say goodbye
    scope: [src/greet.rs, tests/greet.rs]
    depends_on: [greet]
    criteria:
      - cmd: cargo test --test greet
        proves: the program also says goodbye
      - cmd: cargo test --test greet_alone
        proves: the greeting still reads as it did
        type: guard
    changes:
      - at: src/greet.rs::Greeter::bye
        what: the goodbye, beside the greeting
    uses: [Greeter]
    outcome: "`hello ana` prints a goodbye after the greeting."
    invariants:
      - The greeting reads as it did.
"#;

const SPEC: &str = r#"
specs:
  - task: greet
    files:
      - path: tests/greet.rs
        content: |
          #[test]
          fn the_greeting_names_the_person() {
              let said = run(&["ana"]);
              assert_eq!(said, "hello, ana\n");
          }
    tests:
      - cmd: cargo test --test greet
        proves: the greeting names the person
"#;

/// A criterion of `from`, run by `cmd` and proving `proves`.
fn criterion(cmd: &str, proves: &str, from: HeldTo) -> JudgedCriterion {
    JudgedCriterion {
        cmd: cmd.to_string(),
        proves: Some(proves.to_string()),
        from,
    }
}

/// The command the spec's test of `greet` runs.
const GREET: &str = "cargo test --test greet";

/// A plan of two tasks with what keeps it from being proven: a change on a
/// test the spec wrote, and a task judged by another's test.
pub(super) fn review() -> PlanReview {
    PlanReview {
        plan: yunta_core::yaml::parse(PLAN).unwrap(),
        departed: Vec::new(),
        spec: Some(yunta_core::yaml::parse(SPEC).unwrap()),
        spec_node: Some("specify".into()),
        held: vec!["cargo test".to_string()],
        tasks: vec![greet_review(), farewell_review()],
        handed_over: None,
        answered: vec![Answered {
            id: "greeting-word".into(),
            question: "Which word should the greeting use?".to_string(),
            answer: "hello".to_string(),
        }],
        asked: Some("greet people by their name".to_string()),
        shapes_read: None,
    }
}

/// How the run judges `greet`: by the spec's test, which the task plans to
/// change.
fn greet_review() -> TaskReview {
    TaskReview {
        task: "greet".into(),
        criteria: vec![criterion(
            GREET,
            "the greeting names the person",
            HeldTo::Spec,
        )],
        denied: vec![DeniedChange {
            at: "tests/greet.rs".to_string(),
            owner: "greet".into(),
        }],
        guards: Vec::new(),
    }
}

/// How the run judges `farewell`: by the spec's test of `greet`, with a
/// guard of its own for what it keeps.
fn farewell_review() -> TaskReview {
    TaskReview {
        task: "farewell".into(),
        criteria: vec![criterion(
            GREET,
            "the program also says goodbye",
            HeldTo::AnotherTask {
                task: "greet".into(),
            },
        )],
        denied: Vec::new(),
        guards: vec![criterion(
            "cargo test --test greet_alone",
            "the greeting still reads as it did",
            HeldTo::Plan,
        )],
    }
}

/// A plan of four tasks in three steps, as a planner handed it over for a
/// person to approve: each task held to a criterion of its own and a test
/// the spec wrote, every choice made for the person; and each shape as the
/// tree it was written against held it: two it makes, a signature it
/// widens that five files beyond the plan name, and an enum it changes.
pub(super) fn global_packs() -> PlanReview {
    let plan: TasksFile =
        yunta_core::yaml::parse(include_str!("fixtures/global-packs.tasks.yaml")).unwrap();
    let spec: SpecFile =
        yunta_core::yaml::parse(include_str!("fixtures/global-packs.spec.yaml")).unwrap();
    let tasks = plan
        .tasks
        .iter()
        .map(|task| {
            let test = &spec.of(&task.id).expect("every task has a spec").tests[0];
            let own = &task.criteria[0];
            TaskReview {
                task: task.id.clone(),
                criteria: vec![
                    criterion(&own.cmd, own.proves.as_deref().unwrap(), HeldTo::Plan),
                    criterion(&test.cmd, &test.proves, HeldTo::Spec),
                ],
                denied: Vec::new(),
                guards: Vec::new(),
            }
        })
        .collect();
    PlanReview {
        plan,
        departed: Vec::new(),
        spec: Some(spec),
        spec_node: Some("specify".into()),
        held: vec![
            "cargo clippy --workspace --all-targets -- -D warnings".to_string(),
            "cargo test --workspace".to_string(),
        ],
        tasks,
        handed_over: None,
        answered: Vec::new(),
        asked: Some("add pack global instalation support, not only project level as today".into()),
        shapes_read: Some(
            yunta_core::yaml::parse(include_str!("fixtures/global-packs.read.yaml")).unwrap(),
        ),
    }
}

/// `review` drawn in `form` at a plain terminal.
pub(super) fn read(review: &PlanReview, form: Form) -> String {
    Terminal::on(Look::plain()).draw(&document(review, &run(), form, &*spell()))
}

/// `review` as the plan a gate shows: the run's tasks document, at the path
/// the run holds it.
pub(super) fn shown_plan(review: PlanReview) -> ShownDocument {
    ShownDocument {
        shown: yunta_core::events::Shown {
            producer: Some("plan".into()),
            artifact: yunta_core::events::ArtifactId::Interpreted {
                kind: yunta_core::ArtifactKind::Tasks,
            },
            content_hash: yunta_core::ContentHash::sha256(b"plan"),
        },
        path: PathBuf::from("artifacts/plan/tasks.yaml"),
        content: ShownContent::Tasks(Box::new(review)),
    }
}

/// The lines a plan under review opens on at `environment`, unpainted: its title
/// breaks between facts only where they do not fit, and its summary hangs from
/// it as a gist, wrapped to the width.
fn head_of(environment: &Environment) -> &'static [&'static str] {
    match environment.name {
        "unicode-60" => &[
            "plan: 4 tasks in 3 steps, 4 decided for you, 2 risks,",
            "      2 left out",
            "  Install packs in the user's Yunta state directory for use",
            "  across projects.",
        ],
        "ascii-80" | "unicode-color-100" => &[
            "plan: 4 tasks in 3 steps, 4 decided for you, 2 risks, 2 left out",
            "  Install packs in the user's Yunta state directory for use across projects.",
        ],
        other => panic!("no expected head for the environment {other}"),
    }
}

/// The plan under review, drawn in each environment: it opens on its head and
/// every line fits the width in every environment, then the whole matches its
/// golden in each one.
#[test]
fn a_plan_under_review_matches_its_goldens() {
    let doc = document(&global_packs(), &run(), Form::Review, &*spell());
    for environment in &ENVIRONMENTS {
        let head = head_of(environment);
        let plain = Terminal::on(unpainted(environment)).draw(&doc);
        let opening: Vec<&str> = plain.lines().take(head.len()).collect();
        assert_eq!(opening, head, "{plain}");
        assert_fits(&doc, environment);
    }
    for environment in &ENVIRONMENTS {
        let drawn = Terminal::on(Look::of(environment)).draw(&doc);
        assert_golden(&environment.golden(&goldens(), "plan-review"), &drawn);
    }
}

#[test]
fn a_whole_plan_as_a_file_matches_its_golden() {
    let doc = document(&global_packs(), &run(), Form::Whole, &*spell());
    assert_golden(&goldens().join("plan.md"), &Markdown.draw(&doc));
}

/// A person deciding reads the change: no line of a file the spec wrote,
/// each shape once under its file and name, and one `tested by` a task.
#[test]
fn a_plan_under_review_reads_as_its_change_and_its_proof_a_line_a_task() {
    let review = global_packs();
    let drawn = read(&review, Form::Review);
    for line in review
        .spec
        .iter()
        .flat_map(|spec| &spec.specs)
        .flat_map(|spec| &spec.files)
        .flat_map(|file| file.content.lines())
        .map(str::trim)
        .filter(|line| line.len() > 2)
    {
        assert!(!drawn.contains(line), "a test's code `{line}`:\n{drawn}");
    }
    for shape in &review.plan.shapes {
        let title = format!("{} :: {}", shape.file, shape.name);
        assert_eq!(drawn.matches(&title).count(), 1, "`{title}` once:\n{drawn}");
    }
    assert_eq!(drawn.matches("tested by").count(), 4, "{drawn}");
}

/// The head carries every fact it says, and no count of tests: read where
/// no line is too narrow to hold them.
#[test]
fn the_head_says_how_big_the_plan_is_and_what_the_person_asked() {
    let drawn = Markdown.draw(&document(&global_packs(), &run(), Form::Review, &*spell()));
    let head = drawn.lines().next().unwrap_or_default();
    for fact in [
        "plan",
        "4 tasks in 3 steps",
        "4 decided for you",
        "2 risks",
        "2 left out",
    ] {
        assert!(head.contains(fact), "`{fact}` heads the plan: {head}");
    }
    assert!(!head.contains("test"), "{head}");
    assert!(
        drawn.contains("add pack global instalation support, not only project level as today"),
        "{drawn}"
    );
}

#[test]
fn the_steps_name_every_task_whole_and_draw_the_order_they_wait_in() {
    let review = global_packs();
    let look = Look {
        width: Look::plain().width.within(60),
        ..Look::plain()
    };
    let drawn = Terminal::on(look).draw(&document(&review, &run(), Form::Review, &*spell()));
    let steps =
        &drawn[drawn.find("\n  steps\n").expect(&drawn)..drawn.find("step 1 of 3").unwrap()];
    let said = steps.split_whitespace().collect::<Vec<_>>().join(" ");
    for task in &review.plan.tasks {
        assert!(
            said.contains(&task.title),
            "`{}` whole:\n{steps}",
            task.title
        );
    }
    assert!(
        steps.contains("| global-pack-cli |"),
        "a box a task:\n{steps}"
    );
    assert!(!steps.contains("..."), "nothing cut:\n{steps}");
}

#[test]
fn a_whole_plan_holds_each_file_the_spec_wrote_after_its_cards_and_its_proof_last() {
    let drawn = read(&global_packs(), Form::Whole);
    let tests = drawn
        .find("fn global_pack_storage_keeps_project_and_user_locks_separate() {")
        .expect("the spec's tests, whole");
    assert!(
        tests > drawn.find("global-pack-docs: Document").unwrap(),
        "{drawn}"
    );
    assert!(drawn.rfind("\n  proof\n").expect(&drawn) > tests, "{drawn}");
    assert!(
        !drawn.contains("--node specify"),
        "tests drawn need no command:\n{drawn}"
    );
}

/// A plan's risks are said once, beside the decision a page puts the plan
/// under: a review of the plan leaves them out, the whole plan draws each one,
/// and its file opens on its title with the summary as its second paragraph.
#[test]
fn forms_say_their_risks_where_their_decision_is() {
    let review = global_packs();
    let reviewed = read(&review, Form::Review);
    assert!(!reviewed.contains("Global lock state"), "{reviewed}");
    let whole = read(&review, Form::Whole);
    assert_eq!(whole.matches("Global lock state").count(), 1, "{whole}");
    let file = Markdown.draw(&document(&review, &run(), Form::Whole, &*spell()));
    assert!(file.starts_with("# plan: "), "{file}");
    let summary = file.split("\n\n").nth(1).unwrap_or_default();
    assert_eq!(
        summary, "Install packs in the user's Yunta state directory for use across projects.",
        "{file}"
    );
}

#[test]
fn the_proof_names_where_a_review_reads_the_spec_s_tests_whole() {
    let mut review = global_packs();
    let drawn = read(&review, Form::Review);
    let proof = &drawn[drawn.rfind("\n  proof\n").unwrap()..];
    assert!(proof.contains("4 in 4 files"), "{proof}");
    assert!(
        proof.contains("yunta status 7E5PH4 --node specify"),
        "{proof}"
    );
    assert!(proof.contains("cargo test --workspace"), "{proof}");
    review.spec_node = None;
    assert!(
        !read(&review, Form::Review).contains("--node"),
        "no node, no command"
    );
}

#[test]
fn the_proof_says_each_guard_and_what_no_guard_checks() {
    let mut review = review();
    let drawn = read(&review, Form::Review);
    let proof = &drawn[drawn.rfind("\n  proof\n").unwrap()..];
    assert!(
        proof.contains("`farewell`: the greeting still reads as it did"),
        "{proof}"
    );
    assert!(proof.contains("cargo test --test greet_alone"), "{proof}");
    assert!(
        proof.contains("nothing but the suite checks 1 promise"),
        "{proof}"
    );
    review.held.clear();
    let drawn = read(&review, Form::Review);
    assert!(
        drawn.contains("no guard of theirs checks 1 promise"),
        "{drawn}"
    );
}

#[test]
fn what_keeps_a_plan_from_being_proven_is_counted_in_its_head_and_said_in_its_proof() {
    let flaw = "`greet` plans to change `tests/greet.rs`, a test the spec wrote";
    let reviewed = read(&review(), Form::Review);
    assert!(
        reviewed.contains("2 problems keep this plan from being proven"),
        "{reviewed}"
    );
    assert!(
        !reviewed.contains(flaw),
        "said beside the decision:\n{reviewed}"
    );
    let whole = read(&review(), Form::Whole);
    let proof = whole.rfind("\n  proof\n").expect("a proof section");
    assert!(whole[proof..].contains(flaw), "{whole}");
}

#[test]
fn the_description_is_read_whole_with_its_drawing() {
    let drawn = read(&review(), Form::Review);
    assert!(
        drawn.contains("| Name |") && drawn.contains("| Greeting |"),
        "{drawn}"
    );
    assert!(
        drawn.contains("A goodbye follows the greeting, on its own line."),
        "{drawn}"
    );
}

/// A spec's page shows a long file to its cut, and where the rest is: the
/// page of the node that wrote it. With no node, the file is shown whole.
#[test]
fn a_spec_page_cuts_a_long_file_and_says_where_the_rest_is() {
    let mut spec: SpecFile = yunta_core::yaml::parse(SPEC).unwrap();
    spec.specs[0].files[0].content = (1..=80).map(|n| format!("// line {n}\n")).collect();
    let node: yunta_core::NodeId = "specify".into();
    let draw = |node: Option<&yunta_core::NodeId>| {
        let doc = crate::spec::document(&spec, ("", node), &run(), Form::Review, &*spell());
        Terminal::on(Look::plain()).draw(&doc)
    };
    let cut = draw(Some(&node));
    assert!(
        cut.contains("// line 60") && !cut.contains("// line 61"),
        "{cut}"
    );
    assert!(
        cut.contains("20 lines more: yunta status 7E5PH4 --node specify"),
        "{cut}"
    );
    assert!(draw(None).contains("// line 80"), "no node, the file whole");
}
