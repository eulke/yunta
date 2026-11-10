use yunta_core::{ConfigLayer, Detected};

use super::config_yaml;

/// The name a project is written under, as its config reads it back.
fn read_back(name: &str) -> Option<String> {
    let yaml = config_yaml(name, "main", &[], &Detected::default());
    ConfigLayer::read(&yaml).unwrap().project.unwrap().name
}

/// A project's name reads back whole, whatever a directory is called: one a
/// YAML comment or a mapping would cut is quoted.
#[test]
fn a_project_name_reads_back_whole() {
    for name in [
        "my #project",
        "a: b",
        "'quoted'",
        "\"double\"",
        "yes",
        "042",
    ] {
        assert_eq!(read_back(name).as_deref(), Some(name), "{name}");
    }
}

/// A name nothing in YAML would cut is written as it is.
#[test]
fn a_plain_name_is_written_as_it_is() {
    let yaml = config_yaml("web-app", "main", &[], &Detected::default());
    assert!(yaml.contains("\n  name: web-app\n"), "{yaml}");
    assert!(yaml.contains("\n  base_branch: main\n"), "{yaml}");
}
