//! The YAML frontier names the value that failed, wherever it sits.

use serde::Deserialize;
use yunta_core::yaml::{self, YamlError};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Doc {
    #[allow(dead_code)]
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    count: u32,
}

#[test]
fn a_parse_error_names_the_path_of_the_failing_value() {
    let err =
        yaml::parse::<Doc>("items:\n  - name: a\n    count: 1\n  - name: b\n    count: many\n")
            .unwrap_err();
    match &err {
        YamlError::Parse { path, .. } => assert_eq!(path, "items[1].count"),
        other => panic!("the error must locate the value, got {other:?}"),
    }
}

#[test]
fn an_unknown_key_is_named_with_the_keys_that_are_valid_there() {
    let err =
        yaml::parse::<Doc>("items:\n  - name: a\n    count: 1\n    colour: red\n").unwrap_err();
    assert_eq!(
        err.to_string(),
        "`items[0].colour`: items[0]: unknown field `colour`, expected `name` or `count` at line 4 column 5"
    );
}

#[test]
fn a_value_parsed_after_buffering_still_names_its_path() {
    let value: yaml::Value =
        yaml::parse("items:\n  - name: a\n    count: 1\n  - name: b\n    count: many\n").unwrap();
    let err = yaml::from_value::<Doc>(value).unwrap_err();
    match &err {
        YamlError::Parse { path, .. } => assert_eq!(path, "items[1].count"),
        other => panic!("expected a parse error, got {other:?}"),
    }
}

#[test]
fn bytes_that_are_not_utf8_are_reported_as_such() {
    let err = yaml::parse_bytes::<Doc>(&[0xff, 0xfe, b'a']).unwrap_err();
    assert!(matches!(err, YamlError::Utf8 { .. }), "{err:?}");
}

#[test]
fn a_document_round_trips_through_to_string() {
    #[derive(Debug, PartialEq, serde::Serialize, Deserialize)]
    struct Pair {
        left: String,
        right: u8,
    }
    let pair = Pair {
        left: "l".into(),
        right: 2,
    };
    let text = yaml::to_string(&pair).unwrap();
    assert_eq!(yaml::parse::<Pair>(&text).unwrap(), pair);
}
