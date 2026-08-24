use super::*;

const REDACTED_SECRET: &str = "[REDACTED_SECRET]";

/// Remove granted credential values before framework-controlled text reaches
/// logs, Surface errors, or the canvas.
pub(super) fn redact_framework_text(
    mut text: String,
    credentials: &[loom_protocol::CredentialGrant],
) -> String {
    for credential in credentials {
        if !credential.value.is_empty() && text.contains(&credential.value) {
            text = text.replace(&credential.value, REDACTED_SECRET);
        }
    }
    text
}

pub(super) fn redact_framework_error(
    error: ToolRegistryError,
    credentials: &[loom_protocol::CredentialGrant],
) -> ToolRegistryError {
    match error {
        ToolRegistryError::FrameworkProcessSpawn {
            id,
            framework,
            reason,
        } => ToolRegistryError::FrameworkProcessSpawn {
            id,
            framework,
            reason: redact_framework_text(reason, credentials),
        },
        ToolRegistryError::FrameworkProcessIo {
            id,
            framework,
            reason,
        } => ToolRegistryError::FrameworkProcessIo {
            id,
            framework,
            reason: redact_framework_text(reason, credentials),
        },
        ToolRegistryError::FrameworkProcessProtocol {
            id,
            framework,
            reason,
        } => ToolRegistryError::FrameworkProcessProtocol {
            id,
            framework,
            reason: redact_framework_text(reason, credentials),
        },
        ToolRegistryError::FrameworkProcessFailed {
            id,
            framework,
            code,
            message,
            detail,
        } => ToolRegistryError::FrameworkProcessFailed {
            id,
            framework,
            code: redact_framework_text(code, credentials),
            message: redact_framework_text(message, credentials),
            detail: redact_framework_text(detail, credentials),
        },
        error => error,
    }
}
