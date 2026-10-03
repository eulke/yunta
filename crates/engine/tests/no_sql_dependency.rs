//! SQL access is `yunta-storage`'s alone: the engine never depends on
//! `rusqlite` or `sqlx` itself, and this reads its manifest to say so
//! when the dependency list is edited.

#[test]
fn engine_never_depends_on_a_sql_driver_directly() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        !cargo_toml.contains("rusqlite"),
        "yunta-engine must not depend on rusqlite directly — go through yunta-storage instead"
    );
    assert!(
        !cargo_toml.contains("sqlx"),
        "yunta-engine must not depend on sqlx directly — go through yunta-storage instead"
    );
}
