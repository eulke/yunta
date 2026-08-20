//! `resolve_inputs` (T1.5, §2.3, D82): CLI-provided values win, the
//! spec's own `default` fills the rest, and every value — from either
//! source — is validated before it reaches a manifest.

use std::collections::{BTreeMap, HashMap};

use yunta_core::InputSpec;
use yunta_engine::{resolve_inputs, InputsError};

fn specs(yaml: &str) -> BTreeMap<String, InputSpec> {
    #[derive(serde::Deserialize)]
    struct Workflow {
        inputs: BTreeMap<String, InputSpec>,
    }
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    workflow.inputs
}

#[test]
fn a_missing_optional_input_resolves_to_its_default() {
    let specs = specs("inputs:\n  greeting:\n    type: string\n    default: hello\n");
    let resolved = resolve_inputs(&specs, &HashMap::new(), std::path::Path::new(".")).unwrap();
    assert_eq!(resolved["greeting"], "hello");
}

#[test]
fn a_provided_value_overrides_the_default() {
    let specs = specs("inputs:\n  greeting:\n    type: string\n    default: hello\n");
    let provided = HashMap::from([("greeting".to_string(), "hola".to_string())]);
    let resolved = resolve_inputs(&specs, &provided, std::path::Path::new(".")).unwrap();
    assert_eq!(resolved["greeting"], "hola");
}

#[test]
fn a_required_input_with_no_value_fails_naming_it() {
    let specs = specs("inputs:\n  idea:\n    type: string\n    required: true\n");
    let err = resolve_inputs(&specs, &HashMap::new(), std::path::Path::new(".")).unwrap_err();
    assert_eq!(
        err,
        InputsError::Missing {
            name: "idea".to_string()
        }
    );
}

#[test]
fn an_undeclared_provided_input_is_an_error() {
    let specs = specs("inputs: {}\n");
    let provided = HashMap::from([("ghost".to_string(), "x".to_string())]);
    let err = resolve_inputs(&specs, &provided, std::path::Path::new(".")).unwrap_err();
    assert_eq!(
        err,
        InputsError::Unknown {
            name: "ghost".to_string()
        }
    );
}

#[test]
fn a_number_input_parses_and_enforces_min_and_max() {
    let specs = specs("inputs:\n  n:\n    type: number\n    min: 1\n    max: 10\n");

    let low = HashMap::from([("n".to_string(), "0".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &low, std::path::Path::new(".")),
        Err(InputsError::BelowMin { .. })
    ));

    let high = HashMap::from([("n".to_string(), "11".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &high, std::path::Path::new(".")),
        Err(InputsError::AboveMax { .. })
    ));

    let not_a_number = HashMap::from([("n".to_string(), "banana".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &not_a_number, std::path::Path::new(".")),
        Err(InputsError::InvalidNumber { .. })
    ));

    let ok = HashMap::from([("n".to_string(), "5".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &ok, std::path::Path::new(".")).unwrap()["n"],
        "5"
    );
}

#[test]
fn an_integer_default_renders_without_a_trailing_decimal() {
    let specs = specs("inputs:\n  max_tasks:\n    type: number\n    default: 40\n");
    let resolved = resolve_inputs(&specs, &HashMap::new(), std::path::Path::new(".")).unwrap();
    assert_eq!(resolved["max_tasks"], "40");
}

#[test]
fn a_boolean_input_only_accepts_true_or_false() {
    let specs = specs("inputs:\n  dry_run:\n    type: boolean\n    default: false\n");

    let ok = HashMap::from([("dry_run".to_string(), "true".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &ok, std::path::Path::new(".")).unwrap()["dry_run"],
        "true"
    );

    let bad = HashMap::from([("dry_run".to_string(), "yes".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &bad, std::path::Path::new(".")),
        Err(InputsError::InvalidBoolean { .. })
    ));
}

#[test]
fn an_enum_input_only_accepts_a_declared_value() {
    let specs = specs(
        "inputs:\n  severity_floor:\n    type: enum\n    values: [blocking, major, minor]\n    default: major\n",
    );

    let ok = HashMap::from([("severity_floor".to_string(), "blocking".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &ok, std::path::Path::new(".")).unwrap()["severity_floor"],
        "blocking"
    );

    let bad = HashMap::from([("severity_floor".to_string(), "catastrophic".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &bad, std::path::Path::new(".")),
        Err(InputsError::NotInEnum { .. })
    ));
}

#[test]
fn a_string_input_enforces_min_length_and_pattern() {
    let specs = specs(
        "inputs:\n  branch:\n    type: string\n    min_length: 3\n    pattern: \"^[a-z-]+$\"\n    default: main\n",
    );

    let too_short = HashMap::from([("branch".to_string(), "ab".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &too_short, std::path::Path::new(".")),
        Err(InputsError::TooShort { .. })
    ));

    let bad_pattern = HashMap::from([("branch".to_string(), "Not-Valid".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &bad_pattern, std::path::Path::new(".")),
        Err(InputsError::PatternMismatch { .. })
    ));

    let ok = HashMap::from([("branch".to_string(), "feature-x".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &ok, std::path::Path::new(".")).unwrap()["branch"],
        "feature-x"
    );
}

#[test]
fn a_path_input_validates_existence_against_the_given_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CHANGELOG.md"), "").unwrap();

    let specs = specs("inputs:\n  changelog:\n    type: path\n");

    let missing = HashMap::from([("changelog".to_string(), "NOPE.md".to_string())]);
    assert!(matches!(
        resolve_inputs(&specs, &missing, dir.path()),
        Err(InputsError::PathNotFound { .. })
    ));

    let present = HashMap::from([("changelog".to_string(), "CHANGELOG.md".to_string())]);
    assert_eq!(
        resolve_inputs(&specs, &present, dir.path()).unwrap()["changelog"],
        "CHANGELOG.md"
    );
}
