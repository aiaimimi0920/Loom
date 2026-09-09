use std::io::{Read, Write};

use anyhow::{anyhow, Context, Result};
use loom_protocol::{
    parse_capability_runtime_frame, validate_capability_runtime_message, CapabilityRuntimeMessage,
    CAPABILITY_RUNTIME_FRAME_BYTES,
};

pub fn read(reader: &mut impl Read) -> Result<Option<CapabilityRuntimeMessage>> {
    let mut length = [0_u8; 4];
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
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .context("read complete runtime frame")?;
    parse_capability_runtime_frame(&bytes)
        .map(Some)
        .map_err(|error| anyhow!(error))
}

pub fn write(writer: &mut impl Write, message: &CapabilityRuntimeMessage) -> Result<()> {
    validate_capability_runtime_message(message).map_err(|error| anyhow!(error))?;
    let bytes = serde_json::to_vec(message).context("serialize runtime response")?;
    if bytes.is_empty() || bytes.len() > CAPABILITY_RUNTIME_FRAME_BYTES {
        return Err(anyhow!("runtime response exceeds the frame budget"));
    }
    let length = u32::try_from(bytes.len()).context("runtime response length overflow")?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush().context("flush runtime response")
}
