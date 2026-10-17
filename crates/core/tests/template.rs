//! What the one template syntax promises: a variable is replaced
//! wherever it appears, a name outside the closed set is refused where
//! the template is read, and nothing a caller did not define is ever
//! shipped verbatim.

use std::collections::BTreeMap;

use yunta_core::template::{render_template, template_variables, TemplateError, TemplateVar};

fn vars(pairs: &[(TemplateVar, &str)]) -> BTreeMap<TemplateVar, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.clone(), v.to_string()))
        .collect()
}

#[test]
fn a_known_variable_is_replaced_wherever_it_appears() {
    let out = render_template(
        "Write the tasks document to {{run.dir}}/plan.yaml under {{run.dir}}.",
        &vars(&[(TemplateVar::RunDir, "/tmp/run-1")]),
    )
    .unwrap();
    assert_eq!(
        out,
        "Write the tasks document to /tmp/run-1/plan.yaml under /tmp/run-1."
    );
}

#[test]
fn whitespace_inside_the_braces_is_tolerated() {
    let out = render_template("{{ run.dir }}", &vars(&[(TemplateVar::RunDir, "/r")])).unwrap();
    assert_eq!(out, "/r");
}

#[test]
fn an_undefined_variable_is_an_error_naming_it() {
    let err = render_template("path: {{run.dir}}", &vars(&[])).unwrap_err();
    match err {
        TemplateError::Undefined { name } => assert_eq!(name, TemplateVar::RunDir),
        other => panic!("expected Undefined, got {other}"),
    }
}

#[test]
fn a_name_that_is_not_a_variable_is_refused_where_the_template_is_read() {
    // The set is closed, so this is decided before any caller is asked
    // for a value — and the refusal lists what the author could have
    // written instead.
    let err = render_template(
        "path: {{run.directory}}",
        &vars(&[(TemplateVar::RunDir, "/r")]),
    )
    .unwrap_err();
    assert!(
        matches!(&err, TemplateError::Unknown(unknown) if unknown.name == "run.directory"),
        "{err}"
    );
    assert!(err.to_string().contains("`run.dir`"), "{err}");
}

#[test]
fn a_template_error_reads_back_naming_its_variable_as_the_author_wrote_it() {
    let undefined = TemplateError::Undefined {
        name: TemplateVar::Input("idea".parse().unwrap()),
    };
    let json = serde_json::to_value(&undefined).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"undefined": {"name": "inputs.idea"}})
    );
    let parsed: TemplateError = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, undefined);
}

#[test]
fn an_unclosed_template_is_an_error_not_silent_text() {
    let err =
        render_template("path: {{run.dir", &vars(&[(TemplateVar::RunDir, "/r")])).unwrap_err();
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
    assert_eq!(
        names,
        [
            TemplateVar::RunDir,
            TemplateVar::Input("idea".parse().expect("an input name")),
            TemplateVar::RunDir,
        ]
    );
}
