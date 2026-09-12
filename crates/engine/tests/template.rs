use std::collections::BTreeMap;

use yunta_engine::{render_template, template_variables, TemplateError};

fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn a_known_variable_is_replaced_wherever_it_appears() {
    let out = render_template(
        "Write the tasks document to {{run.dir}}/artifacts/plan.yaml under {{run.dir}}.",
        &vars(&[("run.dir", "/tmp/run-1")]),
    )
    .unwrap();
    assert_eq!(
        out,
        "Write the tasks document to /tmp/run-1/artifacts/plan.yaml under /tmp/run-1."
    );
}

#[test]
fn whitespace_inside_the_braces_is_tolerated() {
    let out = render_template("{{ run.dir }}", &vars(&[("run.dir", "/r")])).unwrap();
    assert_eq!(out, "/r");
}

#[test]
fn an_undefined_variable_is_an_error_naming_it() {
    let err = render_template("path: {{run.dir}}", &vars(&[])).unwrap_err();
    match err {
        TemplateError::Undefined { name } => assert_eq!(name, "run.dir"),
        other => panic!("expected Undefined, got {other}"),
    }
}

#[test]
fn an_unclosed_template_is_an_error_not_silent_text() {
    let err = render_template("path: {{run.dir", &vars(&[("run.dir", "/r")])).unwrap_err();
    assert!(matches!(err, TemplateError::Unclosed { .. }));
}

#[test]
fn text_without_templates_passes_through_untouched() {
    let input = "single {braces} and } stray { are just text";
    let out = render_template(input, &vars(&[])).unwrap();
    assert_eq!(out, input);
}

#[test]
fn template_variables_lists_every_occurrence_for_static_check() {
    let names = template_variables("{{run.dir}} and {{inputs.idea}} and {{run.dir}}").unwrap();
    assert_eq!(names, ["run.dir", "inputs.idea", "run.dir"]);
}
