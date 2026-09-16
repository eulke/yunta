//! What the engine's own persisted documents promise. The rule itself
//! lives in the test crate; each crate asserts it of what it declares.

use chrono::{DateTime, Utc};
use yunta_engine::EngineProcessFile;
use yunta_testkit_core::persisted::holds_its_version;

#[test]
fn every_persisted_file_carries_its_version() {
    holds_its_version(EngineProcessFile {
        schema_version: 1,
        engine_pid: 4242u32.try_into().expect("a pid"),
        started_at: DateTime::<Utc>::UNIX_EPOCH,
        process_groups: vec![4243u32.try_into().expect("a pid")],
    });
    holds_its_version(yunta_engine::lock::LockOwner {
        schema_version: 1,
        pid: 4242u32.try_into().expect("a pid"),
        started_at: DateTime::<Utc>::UNIX_EPOCH,
    });
}
