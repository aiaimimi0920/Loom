// Credential encoding variants and recursive result/error redaction.
struct CredentialRedactor {
    encoded_values: Vec<String>,
}

impl CredentialRedactor {
    fn new(credentials: &[CredentialGrant]) -> Self {
        let mut encoded_values = Vec::new();
        for credential in credentials {
            if credential.value.is_empty() {
                continue;
            }
            let bytes = credential.value.as_bytes();
            let percent_encoded = percent_encode_secret(bytes);
            encoded_values.extend([
                credential.value.clone(),
                percent_encoded.clone(),
                percent_encoded.replace("%20", "+"),
                BASE64.encode(bytes),
                BASE64_URL_SAFE.encode(bytes),
            ]);
        }
        encoded_values.retain(|value| !value.is_empty());
        encoded_values.sort_unstable();
        encoded_values.dedup();
        encoded_values.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
        Self { encoded_values }
    }

    fn redact_text(&self, text: &str) -> String {
        self.encoded_values
            .iter()
            .fold(text.to_owned(), |text, value| {
                text.replace(value, "[REDACTED]")
            })
    }

    fn redact_value(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.redact_text(text),
            Value::Array(values) => {
                for value in values {
                    self.redact_value(value);
                }
            }
            Value::Object(values) => {
                let original = std::mem::take(values);
                for (name, mut value) in original {
                    self.redact_value(&mut value);
                    values.insert(self.redact_text(&name), value);
                }
            }
            _ => {}
        }
    }
}

fn percent_encode_secret(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len());
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(*byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
fn redact_credentials(message: String, credentials: &[CredentialGrant]) -> String {
    CredentialRedactor::new(credentials).redact_text(&message)
}
