//! Fixed handshake and two-event subscription over the manifest-pinned TLS channel.

use super::{read_bounded_regular_file, Shared};
use loom_local_channel::{connect, BridgeDiscovery, ClientSocket};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, Instant};
use tungstenite::{Error, Message};

const IO_SLICE: Duration = Duration::from_millis(250);
const WORKFLOW: &str = "loom.hook.workflow.updated";
const CAPABILITIES: &str = "loom.hook.capabilities.updated";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    hook_bridge: Option<BridgeDiscovery>,
}

pub(super) fn run(manifest: &Path, shared: &Shared) -> Result<(), ()> {
    let bytes = read_bounded_regular_file(manifest, 1024 * 1024, "本地能力清单").map_err(|_| ())?;
    let descriptor: Manifest = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if shared.cancelled() {
        return Ok(());
    }
    let mut socket = connect(&descriptor.hook_bridge.ok_or(())?, IO_SLICE).map_err(|_| ())?;
    {
        let mut interrupt = shared.interrupt.lock().map_err(|_| ())?;
        // Stop either sees this handle or its cancellation prevents registration.
        if shared.cancelled() {
            return Ok(());
        }
        *interrupt = Some(socket.get_ref().sock.interrupt_handle().map_err(|_| ())?);
    }
    send(
        &mut socket,
        json!({
            "method": "loom.hook.handshake", "params": {
                "protocolVersion": "loom.hook.v1", "clientId": "loom-desktop",
                "clientVersion": env!("CARGO_PKG_VERSION"),
                "platform": std::env::consts::OS, "transports": ["websocket"]
            }
        }),
    )?;
    await_confirmation(&mut socket, shared, true)?;
    send(
        &mut socket,
        json!({
            "method": "loom.hook.subscribe", "params": {
                "requestId": "desktop-subscribe", "events": [WORKFLOW, CAPABILITIES]
            }
        }),
    )?;
    await_confirmation(&mut socket, shared, false)?;
    {
        let mut state = shared.counters.lock().map_err(|_| ())?;
        if shared.cancelled() {
            return Ok(());
        }
        state.epoch = state.epoch.wrapping_add(1);
        state.connected = true;
    }
    while !shared.cancelled() {
        if let Some(value) = read(&mut socket, Instant::now() + IO_SLICE)? {
            apply_event(shared, &value);
        }
    }
    Ok(())
}

fn send(socket: &mut ClientSocket, value: Value) -> Result<(), ()> {
    socket
        .get_mut()
        .sock
        .set_operation_deadline(Instant::now() + IO_SLICE);
    socket
        .send(Message::Text(value.to_string()))
        .map_err(|_| ())
}

fn read(socket: &mut ClientSocket, deadline: Instant) -> Result<Option<Value>, ()> {
    socket.get_mut().sock.set_operation_deadline(deadline);
    match socket.read() {
        Ok(Message::Text(text)) => serde_json::from_str(&text).map(Some).map_err(|_| ()),
        Ok(Message::Ping(_)) => {
            // tungstenite queues Pong; flush it even if the peer sends no next frame.
            socket.flush().map_err(|_| ())?;
            Ok(None)
        }
        Ok(Message::Pong(_)) => Ok(None),
        Err(Error::Io(error))
            if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
        {
            Ok(None)
        }
        _ => Err(()),
    }
}

fn await_confirmation(
    socket: &mut ClientSocket,
    shared: &Shared,
    handshake: bool,
) -> Result<(), ()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !shared.cancelled() && Instant::now() < deadline {
        let Some(value) = read(socket, deadline.min(Instant::now() + IO_SLICE))? else {
            continue;
        };
        // Events have no response envelope. Preserve fixed events queued before ack;
        // the confirmed epoch still forces a snapshot refresh for the disconnect gap.
        if !handshake
            && matches!(
                value.get("method").and_then(Value::as_str),
                Some(WORKFLOW | CAPABILITIES)
            )
        {
            apply_event(shared, &value);
            continue;
        }
        if value.get("protocolVersion").and_then(Value::as_str) != Some("loom.hook.v1") {
            return Err(());
        }
        if handshake {
            if value
                .get("sessionId")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty())
                && value.get("transport").and_then(Value::as_str) == Some("websocket")
            {
                return Ok(());
            }
            return Err(());
        }
        if value.get("requestId").and_then(Value::as_str) == Some("desktop-subscribe") {
            let events = value
                .pointer("/data/events")
                .and_then(Value::as_array)
                .ok_or(())?;
            return if value.get("status").and_then(Value::as_str) == Some("succeeded")
                && events.len() == 2
                && [WORKFLOW, CAPABILITIES]
                    .iter()
                    .all(|event| events.iter().any(|v| v.as_str() == Some(event)))
            {
                Ok(())
            } else {
                Err(())
            };
        }
    }
    Err(())
}

pub(super) fn apply_event(shared: &Shared, value: &Value) {
    let method = value.get("method").and_then(Value::as_str);
    let workflow = value.pointer("/params/workflowId");
    let mut state = shared.counters.lock().unwrap_or_else(|e| e.into_inner());
    match method {
        Some(WORKFLOW)
            if workflow.is_none() || workflow.and_then(Value::as_str) == Some("hook-live") =>
        {
            state.workflow = state.workflow.wrapping_add(1);
        }
        Some(CAPABILITIES) => state.capabilities = state.capabilities.wrapping_add(1),
        _ => {}
    }
}
