// Idle peers must not retain one of the bounded native bridge worker slots forever.
const HOOK_BRIDGE_PING_INTERVAL: Duration = Duration::from_secs(30);
const HOOK_BRIDGE_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const HOOK_BRIDGE_READ_SLICE: Duration = Duration::from_millis(100);
const HOOK_BRIDGE_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

struct HookBridgeLiveness {
    last_received: std::time::Instant,
    last_ping: std::time::Instant,
}

impl HookBridgeLiveness {
    fn new(now: std::time::Instant) -> Self {
        Self {
            last_received: now,
            last_ping: now,
        }
    }

    fn received(&mut self, now: std::time::Instant) {
        self.last_received = now;
    }

    fn maintain(
        &mut self,
        socket: &mut loom_local_channel::ServerSocket,
        now: std::time::Instant,
    ) -> bool {
        if now.duration_since(self.last_received) >= HOOK_BRIDGE_IDLE_TIMEOUT {
            return false;
        }
        if now.duration_since(self.last_ping) >= HOOK_BRIDGE_PING_INTERVAL {
            if send_hook_bridge_message(socket, tungstenite::Message::Ping(Vec::new())).is_err() {
                return false;
            }
            self.last_ping = now;
        }
        true
    }
}

fn send_hook_bridge_message(
    socket: &mut loom_local_channel::ServerSocket,
    message: tungstenite::Message,
) -> tungstenite::Result<()> {
    // Renew after synchronous dispatch: its execution time is not an I/O budget.
    socket
        .get_mut()
        .sock
        .set_operation_deadline(std::time::Instant::now() + HOOK_BRIDGE_WRITE_TIMEOUT);
    socket.send(message)
}

fn read_hook_bridge_message(
    socket: &mut loom_local_channel::ServerSocket,
) -> tungstenite::Result<tungstenite::Message> {
    // Absolute rather than per-read: continuous incomplete fragments must yield
    // to cancellation, broadcasts and the idle deadline too.
    socket
        .get_mut()
        .sock
        .set_operation_deadline(std::time::Instant::now() + HOOK_BRIDGE_READ_SLICE);
    socket.read()
}
