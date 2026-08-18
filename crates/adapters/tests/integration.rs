#[test]
fn depends_on_core() {
    assert_eq!(yunta_adapters::depends_on(), "yunta-core");
}
