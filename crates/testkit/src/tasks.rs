//! A tasks document, as a test writes one.

use yunta_core::TasksFile;

/// The document declaring one task per `(id, scope, criterion)` triple,
/// in the order given, read through the same door a run reads one
/// through — so a test states a document in the author's vocabulary and
/// gets back the value the engine works on.
///
/// Each task's scope is a single path and its criteria a single `cmd`,
/// which is everything a test about registration, crossing or re-plan
/// needs to tell two tasks apart.
pub fn tasks_document(tasks: &[(&str, &str, &str)]) -> TasksFile {
    let mut yaml = String::from("tasks:\n");
    for (id, scope, criterion) in tasks {
        yaml.push_str(&format!("  - id: {id}\n"));
        yaml.push_str(&format!("    title: \"{id}\"\n"));
        yaml.push_str(&format!("    scope: [\"{scope}\"]\n"));
        yaml.push_str(&format!("    criteria: [{{cmd: \"{criterion}\"}}]\n"));
    }
    yunta_core::shape::read(yaml.as_bytes(), "tasks").expect("a valid tasks document")
}
