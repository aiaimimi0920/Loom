use std::io::{Read, Write};

use anyhow::{anyhow, Context, Result};
use loom_protocol::{
    parse_capability_runtime_frame, validate_capability_runtime_message, CapabilityRuntimeMessage,
    CAPABILITY_RUNTIME_FRAME_BYTES,
};

/// Reads one bounded big-endian length-prefixed runtime message.
pub fn read<R: Read>(reader: &mut R) -> Result<Option<CapabilityRuntimeMessage>> {
    let mut length = [0u8; 4];
    match reader.read(&mut length[..1]).context("read frame prefix")? {
        0 => return Ok(None),
        1 => reader
            .read_exact(&mut length[1..])
            .context("read complete frame prefix")?,
        _ => unreachable!("one-byte read returned more than one byte"),
    }
    let length = usize::try_from(u32::from_be_bytes(length)).context("frame length overflow")?;
    if length == 0 || length > CAPABILITY_RUNTIME_FRAME_BYTES {
        return Err(anyhow!(
            "runtime frame length is outside the protocol budget"
        ));
    }
    let mut bytes = vec![0u8; length];
    reader
        .read_exact(&mut bytes)
        .context("read complete runtime frame")?;
    parse_capability_runtime_frame(&bytes)
        .map(Some)
        .map_err(|error| anyhow!(error))
}

/// Writes one validated frame without mixing diagnostics into stdout.
pub fn write<W: Write>(writer: &mut W, message: &CapabilityRuntimeMessage) -> Result<()> {
    validate_capability_runtime_message(message).map_err(|error| anyhow!(error))?;
    let bytes = serde_json::to_vec(message).context("serialize runtime response")?;
    if bytes.is_empty() || bytes.len() > CAPABILITY_RUNTIME_FRAME_BYTES {
        return Err(anyhow!("runtime response exceeds the frame budget"));
    }
    let length = u32::try_from(bytes.len()).context("runtime response length overflow")?;
    writer
        .write_all(&length.to_be_bytes())
        .and_then(|_| writer.write_all(&bytes))
        .and_then(|_| writer.flush())
        .context("write runtime response")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use loom_protocol::{
        CapabilityRuntimeMethod, CAPABILITY_API_VERSION, CAPABILITY_RUNTIME_PROTOCOL,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn framed_message_round_trips() {
        let message = CapabilityRuntimeMessage::Request {
            protocol: CAPABILITY_RUNTIME_PROTOCOL.to_owned(),
            api_version: CAPABILITY_API_VERSION.to_owned(),
            request_id: "frame-1".to_owned(),
            method: CapabilityRuntimeMethod::Health,
            payload: json!({}),
        };
        let mut bytes = Vec::new();
        write(&mut bytes, &message).unwrap();
        assert_eq!(read(&mut Cursor::new(bytes)).unwrap(), Some(message));
    }
}
