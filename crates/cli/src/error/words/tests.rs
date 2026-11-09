use super::{project_view, resolve_gate};
use yunta_core::events::ObjectProblem;
use yunta_core::sha256_hex;
use yunta_engine::{ResolveGateError, RunError, StoreError};

#[test]
fn an_artifact_view_that_cannot_be_written_names_the_object_it_lacks() {
    let hash = sha256_hex(b"report");
    let named = hash.to_string();
    assert_eq!(
        project_view(
            "report.md",
            &StoreError::Object(ObjectProblem::Missing { hash }),
        ),
        format!(
            "failed to write the `artifacts/` view of `report.md`: the run holds no object \
             `{named}`: an artifact names bytes that are not under `objects/`"
        )
    );
}

#[test]
fn a_decision_whose_documents_cannot_be_read_names_the_object_once() {
    let run = RunError::Object(ObjectProblem::Missing {
        hash: sha256_hex(b"plan"),
    });
    let said = resolve_gate(&ResolveGateError::Documents(Box::new(run)));
    assert!(
        said.starts_with(
            "the documents the decision shows cannot be read: the run holds no object `"
        ),
        "{said}"
    );
    assert_eq!(said.matches("the run holds no object").count(), 1, "{said}");
}
