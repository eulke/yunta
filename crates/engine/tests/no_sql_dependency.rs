// The acceptance criterion: "el engine no importa rusqlite/sqlx
// directamente" — SQL access is entirely yunta-storage's job.

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
