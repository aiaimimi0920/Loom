use std::io::{self, Read, Write};

use serde_json::{json, Value};

const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const PROTOCOL: &str = "loom.capability.runtime.v1";

fn read_frame(input: &mut impl Read) -> io::Result<Option<Value>> {
    let mut length = [0_u8; 4];
    match input.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame exceeds 4 MiB"));
    }
    let mut bytes = vec![0_u8; length];
    input.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map(Some).map_err(io::Error::other)
}

fn write_frame(output: &mut impl Write, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame exceeds 4 MiB"));
    }
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(&bytes)?;
    output.flush()
}

fn response(request: &Value) -> Value {
    let request_id = request.get("requestId").and_then(Value::as_str).unwrap_or("invalid");
    let text = request
        .pointer("/payload/input/text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({
        "type": "response",
        "protocol": PROTOCOL,
        "apiVersion": "1.0",
        "requestId": request_id,
        "status": "succeeded",
        "payload": {
            "output": { "templateLanguage": "rust", "text": text },
            "effects": []
        }
    })
}

fn run() -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    while let Some(request) = read_frame(&mut input)? {
        let deactivate = request.get("method").and_then(Value::as_str) == Some("deactivate");
        write_frame(&mut output, &response(&request))?;
        if deactivate {
            break;
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("capability runtime failed: {error}");
        std::process::exit(1);
    }
}
