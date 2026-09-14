//! `resolve_inputs`: CLI-provided values win, the
//! spec's own `default` fills the rest, and every value — from either
//! source — is validated before it reaches a manifest.
//!
//! A `document` input resolves to more than a value: the file is read as
//! the kind it declares, refused here with every problem it has, and
//! becomes the artifact the run is born holding — whose identity is what
//! the manifest freezes.

use std::collections::{BTreeMap, HashMap};

use yunta_core::events::ArtifactId;
use yunta_core::{ArtifactKind, InputName, InputSpec};
use yunta_engine::{resolve_inputs, BirthOrigin, InputsError};

fn specs(yaml: &str) -> BTreeMap<InputName, InputSpec> {
    #[derive(serde::Deserialize)]
    struct Workflow {
        inputs: BTreeMap<InputName, InputSpec>,
    }
    let workflow: Workflow = serde_norway::from_str(yaml).unwrap();
    workflow.inputs
}

#[tokio::test]
async fn a_missing_optional_input_resolves_to_its_default() {
    let specs = specs("inputs:\n  greeting:\n    type: string\n    default: hello\n");
    let resolved = resolve_inputs(&specs, &HashMap::new(), std::path::Path::new("."))
        .await
        .unwrap();
    assert_eq!(resolved.values["greeting"], "hello");
}

#[tokio::test]
async fn a_provided_value_overrides_the_default() {
    let specs = specs("inputs:\n  greeting:\n    type: string\n    default: hello\n");
    let provided = HashMap::from([("greeting".into(), "hola".to_string())]);
    let resolved = resolve_inputs(&specs, &provided, std::path::Path::new("."))
        .await
        .unwrap();
    assert_eq!(resolved.values["greeting"], "hola");
}

#[tokio::test]
async fn a_required_input_with_no_value_fails_naming_it() {
    let specs = specs("inputs:\n  idea:\n    type: string\n    required: true\n");
    let err = resolve_inputs(&specs, &HashMap::new(), std::path::Path::new("."))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        InputsError::Missing {
            name: "idea".into()
        }
    );
}

#[tokio::test]
async fn an_undeclared_provided_input_is_an_error() {
    let specs = specs("inputs: {}\n");
    let provided = HashMap::from([("ghost".into(), "x".to_string())]);
    let err = resolve_inputs(&specs, &provided, std::path::Path::new("."))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        InputsError::Unknown {
            names: vec!["ghost".into()],
            declared: vec![]
        }
    );
}

#[tokio::test]
async fn nan_is_rejected() {
    let specs = specs("inputs:\n  n:\n    type: number\n");
    for raw in ["NaN", "nan", "inf", "-inf", "infinity", "+Infinity"] {
        let provided = HashMap::from([("n".into(), raw.to_string())]);
        assert!(
            matches!(
                resolve_inputs(&specs, &provided, std::path::Path::new(".")).await,
                Err(InputsError::InvalidNumber { .. })
            ),
            "`{raw}` is not a finite number"
        );
    }
}

#[tokio::test]
async fn every_unknown_input_is_named_in_order() {
    let specs = specs("inputs:\n  idea:\n    type: string\n    default: x\n");
    let provided = HashMap::from([
        ("zeta".into(), "1".to_string()),
        ("alpha".into(), "2".to_string()),
        ("idea".into(), "3".to_string()),
    ]);
    let err = resolve_inputs(&specs, &provided, std::path::Path::new("."))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        InputsError::Unknown {
            names: vec!["alpha".into(), "zeta".into()],
            declared: vec!["idea".into()]
        }
    );
    let text = err.to_string();
    assert!(
        text.contains("alpha") && text.contains("zeta") && text.contains("idea"),
        "every unknown name and the declared ones are listed: {text}"
    );
}

#[tokio::test]
async fn a_number_input_parses_and_enforces_min_and_max() {
    let specs = specs("inputs:\n  n:\n    type: number\n    min: 1\n    max: 10\n");

    let low = HashMap::from([("n".into(), "0".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &low, std::path::Path::new(".")).await,
        Err(InputsError::BelowMin { .. })
    ));

    let high = HashMap::from([("n".into(), "11".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &high, std::path::Path::new(".")).await,
        Err(InputsError::AboveMax { .. })
    ));

    let not_a_number = HashMap::from([("n".into(), "banana".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &not_a_number, std::path::Path::new(".")).await,
        Err(InputsError::InvalidNumber { .. })
    ));

    let ok = HashMap::from([("n".into(), "5".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &ok, std::path::Path::new("."))
            .await
            .unwrap()
            .values["n"],
        "5"
    );
}

#[tokio::test]
async fn an_integer_default_renders_without_a_trailing_decimal() {
    let specs = specs("inputs:\n  max_tasks:\n    type: number\n    default: 40\n");
    let resolved = resolve_inputs(&specs, &HashMap::new(), std::path::Path::new("."))
        .await
        .unwrap();
    assert_eq!(resolved.values["max_tasks"], "40");
}

#[tokio::test]
async fn a_boolean_input_only_accepts_true_or_false() {
    let specs = specs("inputs:\n  dry_run:\n    type: boolean\n    default: false\n");

    let ok = HashMap::from([("dry_run".into(), "true".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &ok, std::path::Path::new("."))
            .await
            .unwrap()
            .values["dry_run"],
        "true"
    );

    let bad = HashMap::from([("dry_run".into(), "yes".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &bad, std::path::Path::new(".")).await,
        Err(InputsError::InvalidBoolean { .. })
    ));
}

#[tokio::test]
async fn an_enum_input_only_accepts_a_declared_value() {
    let specs = specs(
        "inputs:\n  severity_floor:\n    type: enum\n    values: [blocking, major, minor]\n    default: major\n",
    );

    let ok = HashMap::from([("severity_floor".into(), "blocking".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &ok, std::path::Path::new("."))
            .await
            .unwrap()
            .values["severity_floor"],
        "blocking"
    );

    let bad = HashMap::from([("severity_floor".into(), "catastrophic".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &bad, std::path::Path::new(".")).await,
        Err(InputsError::NotInEnum { .. })
    ));
}

#[tokio::test]
async fn a_string_input_enforces_min_length_and_pattern() {
    let specs = specs(
        "inputs:\n  branch:\n    type: string\n    min_length: 3\n    pattern: \"^[a-z-]+$\"\n    default: main\n",
    );

    let too_short = HashMap::from([("branch".into(), "ab".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &too_short, std::path::Path::new(".")).await,
        Err(InputsError::TooShort { .. })
    ));

    let bad_pattern = HashMap::from([("branch".into(), "Not-Valid".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &bad_pattern, std::path::Path::new(".")).await,
        Err(InputsError::PatternMismatch { .. })
    ));

    let ok = HashMap::from([("branch".into(), "feature-x".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &ok, std::path::Path::new("."))
            .await
            .unwrap()
            .values["branch"],
        "feature-x"
    );
}

#[tokio::test]
async fn a_path_input_validates_existence_against_the_given_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CHANGELOG.md"), "").unwrap();

    let specs = specs("inputs:\n  changelog:\n    type: path\n");

    let missing = HashMap::from([("changelog".into(), "NOPE.md".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &missing, dir.path()).await,
        Err(InputsError::PathNotFound { .. })
    ));

    let present = HashMap::from([("changelog".into(), "CHANGELOG.md".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &present, dir.path())
            .await
            .unwrap()
            .values["changelog"],
        "CHANGELOG.md"
    );
}

#[tokio::test]
async fn a_document_input_validates_existence_against_the_given_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plan.yaml"), "tasks: []\n").unwrap();

    let specs = specs("inputs:\n  plan:\n    type: document\n    kind: tasks\n");

    let missing = HashMap::from([("plan".into(), "NOPE.yaml".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &missing, dir.path()).await,
        Err(InputsError::PathNotFound { .. })
    ));

    assert!(resolve_inputs(
        &specs,
        &HashMap::from([("plan".into(), "plan.yaml".to_string())]),
        dir.path()
    )
    .await
    .is_ok());
}

/// The tasks document every document-input test starts from: valid, and
/// spelled the way a person writes one rather than the way the engine
/// renders it back.
const HAND_WRITTEN_TASKS: &str = r#"
tasks:
  - id:       greeting
    title:    "Add the greeting"
    scope:    ["src/**"]
    criteria: [{cmd: "test -f src/greeting.rs"}]
"#;

fn document_specs() -> BTreeMap<InputName, InputSpec> {
    specs("inputs:\n  tasks:\n    type: document\n    kind: tasks\n")
}

fn given(name: &str, value: &str) -> HashMap<InputName, String> {
    HashMap::from([(name.into(), value.to_string())])
}

#[tokio::test]
async fn a_document_input_becomes_an_artifact_the_run_is_born_holding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plan.yaml"), HAND_WRITTEN_TASKS).unwrap();

    let resolved = resolve_inputs(&document_specs(), &given("tasks", "plan.yaml"), dir.path())
        .await
        .unwrap();

    let [document] = &resolved.documents[..] else {
        panic!("one document entered the run: {:?}", resolved.documents);
    };
    assert_eq!(
        document.artifact,
        ArtifactId::Interpreted {
            kind: ArtifactKind::Tasks
        },
        "a document is identified by its kind, so the input names no file"
    );
    assert_eq!(
        document.origin,
        BirthOrigin::Input {
            input: "tasks".into()
        },
        "the run came by it as the input it was given as"
    );
    assert_eq!(
        String::from_utf8(document.bytes.clone()).unwrap(),
        yunta_core::shape::render(
            &yunta_core::shape::read::<yunta_core::TasksFile>(
                HAND_WRITTEN_TASKS.as_bytes(),
                "plan"
            )
            .unwrap()
        )
        .unwrap(),
        "what the run holds is the canonical rendering of what it read, not the file's own \
         spelling"
    );
}

#[tokio::test]
async fn a_document_input_freezes_the_identity_of_the_document_never_the_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plan.yaml"), HAND_WRITTEN_TASKS).unwrap();

    let resolved = resolve_inputs(&document_specs(), &given("tasks", "plan.yaml"), dir.path())
        .await
        .unwrap();

    assert_eq!(
        resolved.values["tasks"],
        format!(
            "sha256:{}",
            yunta_core::sha256_hex(&resolved.documents[0].bytes)
        ),
        "`{{{{inputs.tasks}}}}` renders the document the run holds; the file it came from is \
         gone by the time anything reads it"
    );
}

#[tokio::test]
async fn a_document_input_that_breaks_its_kind_s_rules_is_refused_with_every_problem_named() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("plan.yaml"),
        r#"
tasks:
  - id: same
    title: "First"
    scope: ["a/**"]
    criteria: [{cmd: "true"}]
  - id: same
    title: "Second"
    scope: ["b/**"]
    depends_on: [ghost]
    criteria: [{cmd: "true"}]
"#,
    )
    .unwrap();

    let error = resolve_inputs(&document_specs(), &given("tasks", "plan.yaml"), dir.path())
        .await
        .unwrap_err();

    let InputsError::DocumentRefused { name, report } = &error else {
        panic!("a document that breaks its rules is refused as one: {error:?}");
    };
    assert_eq!(name, "tasks");
    assert_eq!(report.document.path, "plan.yaml", "the file a reader opens");
    assert_eq!(
        report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code())
            .collect::<Vec<_>>(),
        vec!["duplicate-id", "unknown-dependency"],
        "every rule the document breaks, so one correction fixes them all"
    );
}

#[tokio::test]
async fn a_document_input_that_is_not_its_kind_at_all_is_refused_naming_the_path_inside_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plan.yaml"), "tasks:\n  - id: 1\n").unwrap();

    let error = resolve_inputs(&document_specs(), &given("tasks", "plan.yaml"), dir.path())
        .await
        .unwrap_err();

    let InputsError::DocumentRefused { report, .. } = &error else {
        panic!("a document whose shape is wrong is refused as one: {error:?}");
    };
    let text = error.to_string();
    assert!(
        text.contains("plan.yaml") && text.contains("tasks[0]"),
        "the structural problem comes with the path of the value it is about: {text}\n{report}"
    );
}

#[tokio::test]
async fn a_document_input_that_cannot_be_read_as_a_file_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("plan.yaml")).unwrap();

    let error = resolve_inputs(&document_specs(), &given("tasks", "plan.yaml"), dir.path())
        .await
        .unwrap_err();

    assert!(
        matches!(&error, InputsError::DocumentUnreadable { name, .. } if name == "tasks"),
        "a path that is not a readable file is a file problem, not a document one: {error:?}"
    );
}
