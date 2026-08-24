use super::super::*;

#[test]
fn framework_errors_do_not_expose_granted_credential_values() {
    let credentials = vec![loom_protocol::CredentialGrant {
        name: "api_key".to_owned(),
        value: "fixture-secret-value".to_owned(),
        expires_at: None,
    }];
    let error = ToolRegistryError::FrameworkProcessFailed {
        id: "fixture-art".to_owned(),
        framework: "fixture-framework".to_owned(),
        code: "failed".to_owned(),
        message: "request used fixture-secret-value".to_owned(),
        detail: "stderr=fixture-secret-value".to_owned(),
    };

    let redacted = redact_framework_error(error, &credentials);
    let ToolRegistryError::FrameworkProcessFailed {
        message, detail, ..
    } = redacted
    else {
        panic!("redaction changed the framework error variant");
    };
    assert!(!message.contains("fixture-secret-value"));
    assert!(!detail.contains("fixture-secret-value"));
    assert!(message.contains("[REDACTED_SECRET]"));
    assert!(detail.contains("[REDACTED_SECRET]"));
}
