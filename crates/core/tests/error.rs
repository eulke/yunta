use yunta_core::AdapterError;

#[test]
fn unsupported_displays_adapter_and_capability() {
    let err = AdapterError::Unsupported {
        adapter: "mock".into(),
        what: "resume_session",
    };
    assert_eq!(
        err.to_string(),
        "adapter `mock` does not support `resume_session`"
    );
}

#[test]
fn unsupported_is_a_std_error() {
    let err = AdapterError::Unsupported {
        adapter: "mock".into(),
        what: "resume_session",
    };
    let _: &dyn std::error::Error = &err;
}
