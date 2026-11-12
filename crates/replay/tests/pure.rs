//! What replay derives depends on its inputs alone: this reads the crate's
//! manifest to say so when its dependency list is edited.

/// The manifest names no runtime, no store and no engine: the day one is
/// added, this says where the work belongs instead.
#[test]
fn the_crate_reads_nothing_but_its_inputs() {
    let manifest = include_str!("../Cargo.toml");
    for named in ["tokio", "rusqlite", "sqlx", "yunta-storage", "yunta-engine"] {
        assert!(
            !manifest.contains(named),
            "yunta-replay names {named}; what reads a disk, a clock or a process belongs to the engine"
        );
    }
}
