//! The documents a decision shows — a plan as the run judges it, a spec,
//! what a review found — drawn whole on every surface and held to their
//! goldens.

use std::path::{Path, PathBuf};

use yunta_core::events::FindingSeverity;
use yunta_core::shown::{DeniedChange, HeldTo, JudgedCriterion, PlanReview, TaskReview};
use yunta_core::{FindingsFile, SpecFile, TasksFile};
use yunta_testkit_core::golden::{assert_golden, ENVIRONMENTS};

use crate::plan::{document, Form};
use crate::surface::{Markdown, Surface, Terminal};
use crate::Look;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/document")
}

const PLAN: &str = r#"
summary: Greet a person by name, and say goodbye too.
description: |-
  The program says nothing today. It will greet whoever runs it by name, and say goodbye.

  ```mermaid
  graph LR
    Name --> Greeting
  ```
design: |-
  `Greeter` owns the words; `main` reads the name and prints.
decisions:
  - id: default-name
    question: What does it say when no name is given?
    choice: It greets `world`.
    alternatives: [Refuse to run, Ask for a name]
    why: A program run with no arguments should still say something.
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
    changes:
      - at: src/greet.rs::Greeter::bye
        what: the goodbye, beside the greeting
    uses: [Greeter]
    outcome: "`hello ana` prints a goodbye after the greeting."
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

const FINDINGS: &str = r#"
findings:
  - id: newline
    severity: major
    title: A name with a newline breaks the line
    location: src/greet.rs:4
    detail: |-
      `hello` prints the name as given, so `ana\nbob` greets on two lines.
  - id: unwrap
    severity: blocking
    title: An empty argument list panics
    location: src/main.rs:2
    detail: The first argument is read with `unwrap`.
"#;

fn review() -> PlanReview {
    let plan: TasksFile = yunta_core::yaml::parse(PLAN).unwrap();
    let spec: SpecFile = yunta_core::yaml::parse(SPEC).unwrap();
    PlanReview {
        plan,
        departed: Vec::new(),
        spec: Some(spec),
        suite: Some("cargo test".to_string()),
        tasks: vec![
            TaskReview {
                task: "greet".into(),
                criteria: vec![JudgedCriterion {
                    cmd: "cargo test --test greet".to_string(),
                    proves: Some("the greeting names the person".to_string()),
                    from: HeldTo::Spec {
                        file: Some("tests/greet.rs".to_string()),
                    },
                }],
                denied: vec![DeniedChange {
                    at: "tests/greet.rs".to_string(),
                    owner: "greet".into(),
                }],
            },
            TaskReview {
                task: "farewell".into(),
                criteria: vec![JudgedCriterion {
                    cmd: "cargo test --test greet".to_string(),
                    proves: Some("the program also says goodbye".to_string()),
                    from: HeldTo::AnotherTask {
                        task: "greet".into(),
                    },
                }],
                denied: Vec::new(),
            },
        ],
    }
}

#[test]
fn a_plan_under_review_matches_its_goldens() {
    let doc = document(&review(), " of `plan`", "7E5PH4", Form::Review);
    for environment in &ENVIRONMENTS {
        assert_golden(
            &environment.golden(&goldens(), "plan-review"),
            &Terminal::on(Look::of(environment)).draw(&doc),
        );
    }
}

#[test]
fn a_whole_plan_as_a_file_matches_its_golden() {
    let doc = document(&review(), " of `plan`", "7E5PH4", Form::Whole);
    assert_golden(&goldens().join("plan.md"), &Markdown.draw(&doc));
}

#[test]
fn a_task_card_shows_the_code_of_its_changes_and_of_its_test() {
    let doc = document(&review(), " of `plan`", "7E5PH4", Form::Review);
    let drawn = Terminal::on(Look::plain()).draw(&doc);
    let card =
        &drawn[drawn.find("greet — Greet by name").unwrap()..drawn.find("step 2 of 2").unwrap()];
    for code in [
        "| pub struct Greeter {",
        "| fn the_greeting_names_the_person() {",
        "$ cargo test --test greet",
    ] {
        assert!(
            card.contains(code),
            "the card for `greet` lacks `{code}`:\n{card}"
        );
    }
}

#[test]
fn a_change_no_session_may_make_is_said_before_any_task() {
    let doc = document(&review(), " of `plan`", "7E5PH4", Form::Review);
    let drawn = Terminal::on(Look::plain()).draw(&doc);
    let warned = drawn
        .find("1 task plans to change a test the spec wrote")
        .expect("the plan says what cannot be done as written");
    assert!(warned < drawn.find("step 1 of 2").unwrap(), "{drawn}");
}

#[test]
fn a_file_longer_than_a_card_holds_says_where_the_rest_is() {
    let mut review = review();
    let long: String = (1..=80).map(|n| format!("// line {n}\n")).collect();
    if let Some(spec) = review.spec.as_mut() {
        spec.specs[0].files[0].content = long;
    }
    let drawn =
        Terminal::on(Look::plain()).draw(&document(&review, " of `plan`", "7E5PH4", Form::Review));
    assert!(
        drawn.contains("// line 60") && !drawn.contains("// line 61"),
        "{drawn}"
    );
    assert!(
        drawn.contains("20 lines more — yunta status 7E5PH4 --node spec"),
        "{drawn}"
    );
    let whole =
        Terminal::on(Look::plain()).draw(&document(&review, " of `plan`", "7E5PH4", Form::Whole));
    assert!(whole.contains("// line 80"), "{whole}");
}

#[test]
fn a_review_s_findings_match_their_golden() {
    let file: FindingsFile = yunta_core::yaml::parse(FINDINGS).unwrap();
    let doc = crate::findings::document(&file, " of `review`");
    for environment in &ENVIRONMENTS {
        assert_golden(
            &environment.golden(&goldens(), "findings"),
            &Terminal::on(Look::of(environment)).draw(&doc),
        );
    }
    assert_golden(&goldens().join("findings.md"), &Markdown.draw(&doc));
    assert_eq!(
        crate::findings::mark(FindingSeverity::Blocking),
        crate::Mark::Failed
    );
}
