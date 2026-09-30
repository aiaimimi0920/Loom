use std::io;

use anyhow::{anyhow, Result};
use loom_ocr::OcrEngine;
use loom_protocol::{
    CapabilityErrorCode, CapabilityProtocolError, CapabilityRuntimeMessage,
    CapabilityRuntimeMethod, CapabilityRuntimeStatus, CAPABILITY_API_VERSION,
    CAPABILITY_RUNTIME_PROTOCOL,
};
use serde_json::{json, Value};

use crate::commands;
use crate::frame;

#[derive(Default)]
struct RuntimeState {
    active: bool,
    exit_after_response: bool,
    engine: Option<OcrEngine>,
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
            return Err(anyhow!("ocr-host accepts runtime requests only"));
        };
        let result = match method {
            CapabilityRuntimeMethod::Initialize => Ok(json!({ "ready": true })),
            CapabilityRuntimeMethod::Activate => {
                self.active = true;
                Ok(json!({ "active": true }))
            }
            CapabilityRuntimeMethod::Deactivate => {
                self.active = false;
                self.engine = None;
                self.exit_after_response = true;
                Ok(json!({ "active": false }))
            }
            CapabilityRuntimeMethod::Health => Ok(json!({
                "healthy": true,
                "active": self.active,
                "modelLoaded": self.engine.is_some(),
            })),
            CapabilityRuntimeMethod::Cancel => Ok(json!({ "cancelled": true })),
            CapabilityRuntimeMethod::Command if self.active => {
                commands::execute(payload, &mut self.engine)
            }
            CapabilityRuntimeMethod::Command => Err(commands::CommandFailure::new(
                CapabilityErrorCode::PluginNotActive,
                "OCR capability is not active",
                false,
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
                Some(error.into_protocol_error()),
            ),
        })
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
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
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
