// Raw, URL-encoded, and base64 credential redaction contracts.
#[test]
fn credential_values_are_redacted_from_mcp_errors() {
    assert_eq!(
        redact_credentials(
            "server printed secret-value".to_owned(),
            &request().context.credentials
        ),
        "server printed [REDACTED]"
    );
}

#[test]
fn short_credential_values_are_redacted_from_mcp_errors() {
    assert_eq!(
        redact_credentials(
            "server printed key=abc".to_owned(),
            &[CredentialGrant {
                name: "short_key".to_owned(),
                value: "abc".to_owned(),
                expires_at: None,
            }]
        ),
        "server printed key=[REDACTED]"
    );
}

#[test]
fn successful_nested_results_redact_raw_url_and_base64_credentials() {
    let credentials = vec![CredentialGrant {
        name: "api_key".to_owned(),
        value: "secret value/+".to_owned(),
        expires_at: None,
    }];
    let redactor = CredentialRedactor::new(&credentials);
    let mut value = json!({
        "raw": "echo secret value/+",
        "nested": [
            "secret%20value%2F%2B",
            "secret+value%2F%2B",
            BASE64.encode(b"secret value/+")
        ],
        "ordinary": "secret value differs"
    });

    redactor.redact_value(&mut value);

    let encoded = value.to_string();
    assert!(!encoded.contains("secret value/+"));
    assert!(!encoded.contains("secret%20value%2F%2B"));
    assert!(!encoded.contains(&BASE64.encode(b"secret value/+")));
    assert_eq!(value["ordinary"], "secret value differs");
}
