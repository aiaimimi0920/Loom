use std::io::{Read, Write};

use loom_protocol::{
    parse_capability_runtime_frame, validate_capability_runtime_message, CapabilityRuntimeMessage,
    CAPABILITY_RUNTIME_FRAME_BYTES,
};

use crate::error::{CapabilityHostError, HostResult};

/// Writes one `u32` big-endian length-prefixed UTF-8 JSON message.
pub fn write_runtime_frame(
    writer: &mut impl Write,
    message: &CapabilityRuntimeMessage,
) -> HostResult<()> {
    validate_capability_runtime_message(message)
        .map_err(|error| CapabilityHostError::Protocol(error.to_string()))?;
    let bytes = serde_json::to_vec(message)?;
    if bytes.len() > CAPABILITY_RUNTIME_FRAME_BYTES {
        return Err(CapabilityHostError::Protocol(
            "runtime frame exceeds the 4 MiB budget".to_owned(),
        ));
    }
    let length = u32::try_from(bytes.len())
        .map_err(|_| CapabilityHostError::Protocol("runtime frame length overflow".to_owned()))?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

/// Reads exactly one bounded runtime frame and validates its protocol envelope.
pub fn read_runtime_frame(reader: &mut impl Read) -> HostResult<CapabilityRuntimeMessage> {
    let mut length = [0u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > CAPABILITY_RUNTIME_FRAME_BYTES {
        return Err(CapabilityHostError::Protocol(
            "runtime frame exceeds the 4 MiB budget".to_owned(),
        ));
    }
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    parse_capability_runtime_frame(&bytes)
        .map_err(|error| CapabilityHostError::Protocol(error.to_string()))
}
