//! A run's registry written as the engine driving it writes one, for a test that reads the
//! run from outside that engine: this process driving it, asking a person at its terminal
//! or not, or an engine that is gone. A test reads what the registry says through the same
//! probe of the host's processes a reader elsewhere uses, so a registry written here reads
//! alive for as long as this process runs.

use std::path::Path;

use chrono::{DateTime, Utc};
use yunta_core::{NodeId, Pid};
use yunta_engine::lock::SystemProbe;
use yunta_engine::ProcessRegistry;
use yunta_testkit_core::FixedClock;

/// A registry written in a run's directory, kept while this lives; dropped, it is cleared as
/// an engine clears its own at a terminal.
pub struct WrittenRegistry {
    _registry: ProcessRegistry,
}

impl WrittenRegistry {
    /// The registry of an engine in this process driving the run whose directory is
    /// `run_dir`, asking a person at its terminal about the node `asking` names since the
    /// instant it gives, when it names one.
    pub fn driven(run_dir: &Path, asking: Option<(&NodeId, DateTime<Utc>)>) -> WrittenRegistry {
        let registry = written(run_dir, Pid::current());
        if let Some((node, since)) = asking {
            // Built through the wire form the registry keeps a question in, which is how a
            // reader elsewhere meets it.
            let prompt = serde_json::json!({ "node": node.as_str(), "since": since });
            registry.asking(serde_json::from_value(prompt).expect("a question a registry holds"));
        }
        WrittenRegistry {
            _registry: registry,
        }
    }

    /// The registry an engine that died leaves in the run's directory `run_dir`: it names
    /// a process that ended.
    pub fn left(run_dir: &Path) -> WrittenRegistry {
        let mut ended = std::process::Command::new("true")
            .spawn()
            .expect("a process that ends at once");
        let pid = Pid::try_from(ended.id()).expect("a process's id");
        ended.wait().expect("the process ended");
        WrittenRegistry {
            _registry: written(run_dir, pid),
        }
    }
}

/// The registry of the run whose directory is `run_dir`, naming `engine` as the process
/// driving it.
fn written(run_dir: &Path, engine: Pid) -> ProcessRegistry {
    let path = yunta_engine::registry_path(run_dir);
    std::fs::create_dir_all(
        path.parent()
            .expect("a registry stands in the run's scratch"),
    )
    .expect("the run's scratch");
    ProcessRegistry::create(run_dir, engine, &SystemProbe, &FixedClock).expect("the registry")
}
