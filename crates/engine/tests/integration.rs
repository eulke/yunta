#[test]
fn depends_on_storage_and_adapters() {
    assert_eq!(
        yunta_engine::depends_on(),
        ["yunta-storage", "yunta-adapters"]
    );
}

#[test]
fn exposes_version_string() {
    assert!(yunta_engine::version_string().starts_with("yunta-engine "));
}
