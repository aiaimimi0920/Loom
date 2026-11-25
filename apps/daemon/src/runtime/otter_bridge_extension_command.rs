// A connection owns exactly one in-flight command and its cancellation control.
// Keep TLS reads on the connection thread; the scoped worker cannot outlive it.
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
                    // Preserve one ordered follow-up without accepting an unbounded pipeline.
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
