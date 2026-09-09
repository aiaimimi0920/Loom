use std::io;

use anyhow::{anyhow, Result};
use loom_protocol::{
    CapabilityErrorCode, CapabilityProtocolError, CapabilityRuntimeMessage,
    CapabilityRuntimeMethod, CapabilityRuntimeStatus, CAPABILITY_API_VERSION,
    CAPABILITY_RUNTIME_PROTOCOL,
};
use serde_json::{json, Value};

use crate::{command, frame};

#[derive(Default)]
struct RuntimeState {
    lifecycle: Lifecycle,
    exit_after_response: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Lifecycle {
    #[default]
    New,
    Initialized,
    Active,
    Stopped,
}

impl RuntimeState {
    fn handle(&mut self, message: CapabilityRuntimeMessage) -> Result<CapabilityRuntimeMessage> {
        let CapabilityRuntimeMessage::Request {
            request_id,
            method,
            payload,
            ..
        } = message
        else {
            return Err(anyhow!("text translation host accepts requests only"));
        };
        let result = match (method, self.lifecycle) {
            (CapabilityRuntimeMethod::Initialize, Lifecycle::New) => {
                self.lifecycle = Lifecycle::Initialized;
                Ok(json!({ "ready": true }))
            }
            (CapabilityRuntimeMethod::Activate, Lifecycle::Initialized) => {
                self.lifecycle = Lifecycle::Active;
                Ok(json!({ "active": true }))
            }
            (CapabilityRuntimeMethod::Deactivate, Lifecycle::Active) => {
                self.lifecycle = Lifecycle::Stopped;
                self.exit_after_response = true;
                Ok(json!({ "active": false }))
            }
            (CapabilityRuntimeMethod::Health, lifecycle) => Ok(
                json!({ "healthy": lifecycle != Lifecycle::Stopped, "active": lifecycle == Lifecycle::Active }),
            ),
            (CapabilityRuntimeMethod::Cancel, _) => Err(invalid_lifecycle(
                "text translation commands are not cancellable",
            )),
            (CapabilityRuntimeMethod::Command, Lifecycle::Active) => command::execute(payload),
            (CapabilityRuntimeMethod::Command, _) => Err(CapabilityProtocolError {
                code: CapabilityErrorCode::PluginNotActive,
                message: "text translation capability is not active".to_owned(),
                retryable: false,
            }),
            _ => Err(invalid_lifecycle(
                "text translation capability lifecycle request is out of order",
            )),
        };
        Ok(match result {
            Ok(payload) => response(
                request_id,
                CapabilityRuntimeStatus::Succeeded,
                Some(payload),
                None,
            ),
            Err(error) => response(
                request_id,
                CapabilityRuntimeStatus::Failed,
                None,
                Some(error),
            ),
        })
    }
}

fn invalid_lifecycle(message: &str) -> CapabilityProtocolError {
    CapabilityProtocolError {
        code: CapabilityErrorCode::InvalidInput,
        message: message.to_owned(),
        retryable: false,
    }
}

fn response(
    request_id: String,
    status: CapabilityRuntimeStatus,
    payload: Option<Value>,
    error: Option<CapabilityProtocolError>,
) -> CapabilityRuntimeMessage {
    CapabilityRuntimeMessage::Response {
        protocol: CAPABILITY_RUNTIME_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        request_id,
        status,
        payload,
        error,
    }
}

pub fn run() -> Result<()> {
    let (stdin, stdout) = (io::stdin(), io::stdout());
    let (mut reader, mut writer) = (stdin.lock(), stdout.lock());
    let mut state = RuntimeState::default();
    while let Some(message) = frame::read(&mut reader)? {
        let response = state.handle(message)?;
        frame::write(&mut writer, &response)?;
        if state.exit_after_response {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: CapabilityRuntimeMethod, request_id: &str) -> CapabilityRuntimeMessage {
        CapabilityRuntimeMessage::Request {
            protocol: CAPABILITY_RUNTIME_PROTOCOL.to_owned(),
            api_version: CAPABILITY_API_VERSION.to_owned(),
            request_id: request_id.to_owned(),
            method,
            payload: json!({}),
        }
    }

    fn status(message: CapabilityRuntimeMessage) -> CapabilityRuntimeStatus {
        match message {
            CapabilityRuntimeMessage::Response { status, .. } => status,
            _ => panic!("runtime did not return a response"),
        }
    }

    #[test]
    fn enforces_lifecycle_order_and_non_cancellable_contract() {
        let mut state = RuntimeState::default();
        assert_eq!(
            status(
                state
                    .handle(request(CapabilityRuntimeMethod::Activate, "a"))
                    .unwrap()
            ),
            CapabilityRuntimeStatus::Failed
        );
        assert_eq!(
            status(
                state
                    .handle(request(CapabilityRuntimeMethod::Initialize, "i"))
                    .unwrap()
            ),
            CapabilityRuntimeStatus::Succeeded
        );
        assert_eq!(
            status(
                state
                    .handle(request(CapabilityRuntimeMethod::Command, "c"))
                    .unwrap()
            ),
            CapabilityRuntimeStatus::Failed
        );
        assert_eq!(
            status(
                state
                    .handle(request(CapabilityRuntimeMethod::Activate, "a2"))
                    .unwrap()
            ),
            CapabilityRuntimeStatus::Succeeded
        );
        assert_eq!(
            status(
                state
                    .handle(request(CapabilityRuntimeMethod::Cancel, "x"))
                    .unwrap()
            ),
            CapabilityRuntimeStatus::Failed
        );
        assert_eq!(
            status(
                state
                    .handle(request(CapabilityRuntimeMethod::Deactivate, "d"))
                    .unwrap()
            ),
            CapabilityRuntimeStatus::Succeeded
        );
        assert!(state.exit_after_response);
    }
}
