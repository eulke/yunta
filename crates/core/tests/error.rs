use yunta_core::YuntaError;

#[test]
fn unsupported_displays_adapter_and_capability() {
    let err = YuntaError::Unsupported {
        adapter: "mock".to_string(),
        what: "resume_session",
    };
    assert_eq!(
        err.to_string(),
        "adapter `mock` does not support `resume_session`"
    );
}

#[test]
fn unsupported_is_a_std_error() {
    let err = YuntaError::Unsupported {
        adapter: "mock".to_string(),
        what: "resume_session",
    };
    let _: &dyn std::error::Error = &err;
}
