#[test]
fn exposes_crate_identity() {
    assert_eq!(yunta_core::CRATE_NAME, "yunta-core");
}
