//! Test-only, sequential JSON-lines adapter; never shipped as a desktop IPC proxy.
#[path = "bridge_probe/io.rs"]
mod probe_io;

use loom_local_channel::{connect, ClientSocket};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};
use tungstenite::{Error, Message};

const MAX_BYTES: usize = 1024 * 1024;
const MAX_LINE: u64 = 8 * 1024 * 1024;

fn main() {
    if run().is_err() {
        // Never print transport/discovery errors or serialized requests to logs.
        let _ = reply(json!({"ok": false, "error": "probe_operation_failed"}));
        std::process::exit(1);
    }
}

fn reply(value: Value) -> Result<(), ()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &value).map_err(|_| ())?;
    output.write_all(b"\n").map_err(|_| ())?;
    output.flush().map_err(|_| ())
}

fn run() -> Result<(), ()> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or(())?;
    if args.next().is_some() {
        return Err(());
    }
    let identity = probe_io::read_manifest(std::path::Path::new(&path))?;
    let mut socket = connect(&identity, Duration::from_secs(1)).map_err(|_| ())?;
    socket.set_config(|config| {
        config.max_message_size = Some(MAX_BYTES);
        config.max_frame_size = Some(MAX_BYTES);
    });
    reply(json!({"ok":true}))?;
    let mut input = io::stdin().lock();
    loop {
        let Some(command) = read_command(&mut input)? else {
            return Ok(());
        };
        match command.get("op").and_then(Value::as_str) {
            Some("send") => {
                let payload = command.get("payload").and_then(Value::as_str).ok_or(())?;
                if payload.len() > MAX_BYTES {
                    return Err(());
                }
                let _: Value = serde_json::from_str(payload).map_err(|_| ())?;
                socket
                    .get_mut()
                    .sock
                    .set_operation_deadline(deadline(&command)?);
                socket.send(Message::Text(payload.into())).map_err(|_| ())?;
                reply(json!({"ok":true}))?;
            }
            Some("receive") => {
                let max_bytes = command.get("maxBytes").and_then(Value::as_u64).ok_or(())?;
                if !(1..=MAX_BYTES as u64).contains(&max_bytes) {
                    return Err(());
                }
                let payload = receive(&mut socket, deadline(&command)?, max_bytes as usize)?;
                reply(json!({"ok":true, "payload":payload}))?;
            }
            Some("close") => {
                // Drop closes TLS/TCP; do not wait for an untrusted peer's close ack.
                reply(json!({"ok":true}))?;
                return Ok(());
            }
            _ => return Err(()),
        }
    }
}

fn read_command(input: &mut impl BufRead) -> Result<Option<Value>, ()> {
    let mut bytes = Vec::new();
    let count = std::io::Read::take(input, MAX_LINE + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| ())?;
    if count == 0 {
        return Ok(None);
    }
    if count as u64 > MAX_LINE || bytes.last() != Some(&b'\n') {
        return Err(());
    }
    serde_json::from_slice(&bytes).map(Some).map_err(|_| ())
}

fn deadline(command: &Value) -> Result<Instant, ()> {
    let milliseconds = command.get("timeoutMs").and_then(Value::as_u64).ok_or(())?;
    if !(1..=150_000).contains(&milliseconds) {
        return Err(());
    }
    Ok(Instant::now() + Duration::from_millis(milliseconds))
}

fn receive(socket: &mut ClientSocket, deadline: Instant, max_bytes: usize) -> Result<Value, ()> {
    // One deadline covers all fragments and control frames, not each individual read.
    socket.get_mut().sock.set_operation_deadline(deadline);
    loop {
        if Instant::now() >= deadline {
            return Err(());
        }
        match socket.read() {
            Ok(Message::Text(text)) if text.len() <= max_bytes => {
                return serde_json::from_str(&text).map_err(|_| ());
            }
            Ok(Message::Ping(_)) => socket.flush().map_err(|_| ())?,
            Ok(Message::Pong(_)) => {}
            Err(Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(());
            }
            _ => return Err(()),
        }
    }
}

#[cfg(test)]
#[path = "bridge_probe/tests.rs"]
mod tests;
