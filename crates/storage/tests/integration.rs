#[test]
fn depends_on_core() {
    assert_eq!(yunta_storage::depends_on(), "yunta-core");
}
