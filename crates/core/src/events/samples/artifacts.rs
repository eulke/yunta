//! The samples of the artifact kinds: a document written, a document a
//! session submitted, and a document accepted into the run's store.

use crate::events::*;

pub(crate) fn artifact_written() -> ArtifactWrittenPayload {
    ArtifactWrittenPayload {
        path: "artifacts/tasks.yaml".into(),
        content_hash: crate::sha256_hex(b"sha256:111"),
        artifact_kind: Some(crate::ArtifactKind::Tasks),
    }
}

pub(crate) fn artifact_submitted() -> ArtifactSubmittedPayload {
    ArtifactSubmittedPayload {
        name: "plan.yaml".to_string(),
        artifact_kind: crate::ArtifactKind::Tasks,
        outcome: SubmissionOutcome::Accepted {
            content_hash: crate::sha256_hex(b"plan"),
        },
        probes: Vec::new(),
    }
}

pub(crate) fn artifact_accepted() -> ArtifactAcceptedPayload {
    ArtifactAcceptedPayload::new(
        ArtifactId::Interpreted {
            kind: crate::ArtifactKind::Tasks,
        },
        crate::sha256_hex(b"plan"),
        RecordedOrigin::Inherited {
            run: crate::RunId::from("run-parent"),
            producer: Some(crate::NodeId::from("plan")),
        },
    )
}
