// Browsers answer WebSocket Ping automatically. Inbound activity keeps a live
// subscriber connected; outbound traffic alone must not retain a dead peer forever.
const HOOK_BRIDGE_PING_INTERVAL: Duration = Duration::from_secs(30);
const HOOK_BRIDGE_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

struct HookBridgeHeartbeat {
    last_received: Instant,
    last_ping: Instant,
}

impl HookBridgeHeartbeat {
    fn new(now: Instant) -> Self {
        Self {
            last_received: now,
            last_ping: now,
        }
    }

    fn received(&mut self, now: Instant) {
        self.last_received = now;
    }

    fn poll(&mut self, websocket: &mut tungstenite::WebSocket<TcpStream>, now: Instant) -> bool {
        if now.saturating_duration_since(self.last_received) >= HOOK_BRIDGE_IDLE_TIMEOUT {
            let _ = websocket.close(None);
            return false;
        }
        if now.saturating_duration_since(self.last_ping) >= HOOK_BRIDGE_PING_INTERVAL {
            if websocket
                .send(tungstenite::Message::Ping(Vec::new()))
                .is_err()
            {
                return false;
            }
            self.last_ping = now;
        }
        true
    }
}
