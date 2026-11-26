// A connection owns in-flight commands and cancellation control without blocking Ping/Cancel reads.
// Keep TLS reads/writes on the connection thread; scoped workers cannot outlive it.
fn wait_otter_extension_command<T: Send>(
    socket: &mut loom_local_channel::ServerSocket,
    stopped: &AtomicBool,
    deferred: &mut Option<tungstenite::Message>,
    run: impl FnOnce(Arc<loom_capability_runtime::InvocationControl>) -> T + Send,
) -> Option<T> {
    use tungstenite::Message;

    let control = Arc::new(loom_capability_runtime::InvocationControl::new());
    thread::scope(|scope| {
        let worker_control = control.clone();
        let worker = match thread::Builder::new()
            .name("loom-hook-extension-command".to_owned())
            .spawn_scoped(scope, move || run(worker_control))
        {
            Ok(worker) => worker,
            Err(error) => {
                runtime_log_warn(format!("could not start extension command worker: {error}"));
                return None;
            }
        };
        let connected = loop {
            if stopped.load(Ordering::SeqCst) {
                control.cancel();
                let _ = close_otter_bridge_socket(socket, None);
                break false;
            }
            if worker.is_finished() {
                break true;
            }
            match read_otter_bridge_message(socket) {
                Ok(Message::Close(close)) => {
                    control.cancel();
                    let _ = close_otter_bridge_socket(socket, close);
                    break false;
                }
                Ok(Message::Ping(data)) => {
                    if send_otter_bridge_message(socket, Message::Pong(data)).is_err() {
                        break false;
                    }
                }
                Ok(message @ Message::Text(_)) if deferred.is_none() => *deferred = Some(message),
                Ok(Message::Text(_)) => {
                    control.cancel();
                    use tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
                    let _ = close_otter_bridge_socket(
                        socket,
                        Some(CloseFrame {
                            code: CloseCode::Policy,
                            reason: "extension command pipeline is full".into(),
                        }),
                    );
                    break false;
                }
                Ok(_) => {}
                Err(error) if otter_bridge_read_timed_out(&error) => {}
                Err(_) => break false,
            }
        };
        if !connected {
            control.cancel();
        }
        let result = worker.join().map_err(|_| {
            runtime_log_warn("extension command worker panicked");
        });
        if connected {
            result.ok()
        } else {
            None
        }
    })
}

fn is_otter_art_execute_message(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.get("method").and_then(Value::as_str).map(str::to_owned))
        .as_deref()
        == Some(loom_protocol::HOOK_METHOD_ART_EXECUTE)
}

fn wait_otter_art_execute<'a>(
    socket: &mut loom_local_channel::ServerSocket,
    stopped: &AtomicBool,
    deferred: &mut Option<tungstenite::Message>,
    shared_images: &'a SharedImageStoreHandle,
    run: impl FnOnce(&mut dyn FnMut(String)) -> OtterBridgeWebSocketTextResult + Send + 'a,
) -> Option<OtterBridgeWebSocketTextResult> {
    use tungstenite::Message;

    let (intermediate_tx, intermediate_rx) = mpsc::channel::<String>();
    thread::scope(|scope| {
        let worker = match thread::Builder::new()
            .name("loom-otter-art-execute".to_owned())
            .spawn_scoped(scope, move || {
                let mut emit = |msg: String| {
                    let _ = intermediate_tx.send(msg);
                };
                run(&mut emit)
            }) {
            Ok(worker) => worker,
            Err(error) => {
                runtime_log_warn(format!("could not start Otter Art execute worker: {error}"));
                return None;
            }
        };
        let connected = loop {
            while let Ok(intermediate) = intermediate_rx.try_recv() {
                if send_otter_bridge_message(socket, Message::Text(intermediate)).is_err() {
                    return None;
                }
            }
            if stopped.load(Ordering::SeqCst) {
                let _ = close_otter_bridge_socket(socket, None);
                break false;
            }
            if worker.is_finished() {
                break true;
            }
            match read_otter_bridge_message(socket) {
                Ok(Message::Close(close)) => {
                    let _ = close_otter_bridge_socket(socket, close);
                    break false;
                }
                Ok(Message::Ping(data)) => {
                    if send_otter_bridge_message(socket, Message::Pong(data)).is_err() {
                        break false;
                    }
                }
                Ok(Message::Text(next_text)) => {
                    if terminal_relay::try_handle_websocket_relay_result(&next_text) {
                        continue;
                    }
                    match serde_json::from_str::<OtterRequest>(&next_text) {
                        Ok(OtterRequest::ArtCancel(cancel_req)) => {
                            let reply = cancel_otter_art_request(&cancel_req, shared_images);
                            if send_otter_bridge_message(socket, Message::Text(reply)).is_err() {
                                break false;
                            }
                        }
                        Ok(OtterRequest::ArtResourcesRelease(release_req)) => {
                            let reply = release_otter_art_resources(&release_req, shared_images);
                            if send_otter_bridge_message(socket, Message::Text(reply)).is_err() {
                                break false;
                            }
                        }
                        _ if deferred.is_none() => *deferred = Some(Message::Text(next_text)),
                        _ => {
                            use tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
                            let _ = close_otter_bridge_socket(
                                socket,
                                Some(CloseFrame {
                                    code: CloseCode::Policy,
                                    reason: "Otter bridge request pipeline is full".into(),
                                }),
                            );
                            break false;
                        }
                    }
                }
                Ok(_) => {}
                Err(error) if otter_bridge_read_timed_out(&error) => {}
                Err(_) => break false,
            }
        };
        while let Ok(intermediate) = intermediate_rx.try_recv() {
            if connected && send_otter_bridge_message(socket, Message::Text(intermediate)).is_err() {
                return None;
            }
        }
        let result = worker.join().map_err(|_| {
            runtime_log_warn("Otter Art execute worker panicked");
        });
        if connected {
            result.ok()
        } else {
            None
        }
    })
}
