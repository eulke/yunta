//! Where a check diagnostic points, in the words a reader sees. A place is
//! data in `yunta_core`; the words for it are written here, once, and every
//! surface that names a place calls [`label`].

use yunta_core::workflow::{reads::ReadSite, Place};

/// Where a check diagnostic points, in the words a reader sees.
pub fn label(place: &Place) -> String {
    match place {
        Place::Read { node, read } => match read {
            ReadSite::Context => format!("the `artifact:` context source of node `{node}`"),
            ReadSite::Mount => format!("a `mounts:` entry of node `{node}`"),
            ReadSite::Shows => format!("the `shows:` of gate `{node}`"),
        },
        Place::LoopTasks { node } => format!("loop `{node}`"),
        Place::ExternalGate { node } => format!("the external gate of node `{node}`"),
        Place::Field { node, field } => format!("`{field}` of node `{node}`"),
        Place::Distill => "an `on_finish.distill` entry".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::label;
    use yunta_core::workflow::{reads::ReadSite, Place};
    use yunta_core::NodeId;

    #[test]
    fn every_place_keeps_its_wording() {
        let build: NodeId = "build".parse().unwrap();
        let cases = [
            (
                Place::Read {
                    node: build.clone(),
                    read: ReadSite::Context,
                },
                "the `artifact:` context source of node `build`",
            ),
            (
                Place::Read {
                    node: build.clone(),
                    read: ReadSite::Mount,
                },
                "a `mounts:` entry of node `build`",
            ),
            (
                Place::Read {
                    node: build.clone(),
                    read: ReadSite::Shows,
                },
                "the `shows:` of gate `build`",
            ),
            (
                Place::LoopTasks {
                    node: build.clone(),
                },
                "loop `build`",
            ),
            (
                Place::ExternalGate {
                    node: build.clone(),
                },
                "the external gate of node `build`",
            ),
            (
                Place::Field {
                    node: build.clone(),
                    field: "context: node-output".to_string(),
                },
                "`context: node-output` of node `build`",
            ),
            (Place::Distill, "an `on_finish.distill` entry"),
        ];
        for (place, wording) in cases {
            assert_eq!(label(&place), wording);
        }
    }
}
