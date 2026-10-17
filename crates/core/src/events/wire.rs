//! The log's spelling of an artifact's origin.
//!
//! The wire shape of every event kind is generated from the event
//! registry. This file keeps the one shape the registry does not declare:
//! an artifact's origin, which the log writes as a tagged set and the
//! engine reads as an [`ArtifactOrigin`].

use serde::{Deserialize, Serialize};

use super::artifacts::payloads::{ArtifactOrigin, RecordedOrigin};

/// Where an artifact came from, as the log spells it: the recorded
/// origins and `legacy` are one tagged set on the wire, while in Rust a
/// fresh acceptance can only name a [`RecordedOrigin`]. `serde` moves
/// between the two here, so the type the engine holds is not shaped by
/// the file it is written to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ArtifactOriginWire {
    Submitted,
    Ingested,
    Derived,
    Answered,
    Input {
        input: crate::ids::InputName,
    },
    Inherited {
        run: crate::ids::RunId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        producer: Option<crate::ids::NodeId>,
    },
    Legacy,
}

impl From<ArtifactOrigin> for ArtifactOriginWire {
    fn from(origin: ArtifactOrigin) -> Self {
        match origin {
            ArtifactOrigin::Recorded(RecordedOrigin::Submitted) => Self::Submitted,
            ArtifactOrigin::Recorded(RecordedOrigin::Ingested) => Self::Ingested,
            ArtifactOrigin::Recorded(RecordedOrigin::Derived) => Self::Derived,
            ArtifactOrigin::Recorded(RecordedOrigin::Answered) => Self::Answered,
            ArtifactOrigin::Recorded(RecordedOrigin::Input { input }) => Self::Input { input },
            ArtifactOrigin::Recorded(RecordedOrigin::Inherited { run, producer }) => {
                Self::Inherited { run, producer }
            }
            ArtifactOrigin::Legacy => Self::Legacy,
        }
    }
}

impl From<ArtifactOriginWire> for ArtifactOrigin {
    fn from(wire: ArtifactOriginWire) -> Self {
        match wire {
            ArtifactOriginWire::Submitted => Self::Recorded(RecordedOrigin::Submitted),
            ArtifactOriginWire::Ingested => Self::Recorded(RecordedOrigin::Ingested),
            ArtifactOriginWire::Derived => Self::Recorded(RecordedOrigin::Derived),
            ArtifactOriginWire::Answered => Self::Recorded(RecordedOrigin::Answered),
            ArtifactOriginWire::Input { input } => Self::Recorded(RecordedOrigin::Input { input }),
            ArtifactOriginWire::Inherited { run, producer } => {
                Self::Recorded(RecordedOrigin::Inherited { run, producer })
            }
            ArtifactOriginWire::Legacy => Self::Legacy,
        }
    }
}

impl Serialize for ArtifactOrigin {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ArtifactOriginWire::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ArtifactOrigin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ArtifactOriginWire::deserialize(deserializer).map(Into::into)
    }
}

impl schemars::JsonSchema for ArtifactOrigin {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ArtifactOrigin".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ArtifactOriginWire::json_schema(generator)
    }
}
