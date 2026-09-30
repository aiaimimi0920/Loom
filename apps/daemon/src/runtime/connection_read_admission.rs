// Short same-peer bursts wait without occupying the reader reserved for another address.
const CONNECTION_READ_WAIT_MILLIS: u64 = 250;

struct PendingConnectionRead {
    stream: TcpStream,
    peer: IpAddr,
    expires: Instant,
}

#[derive(Default)]
struct ConnectionReadBacklog {
    pending: std::collections::VecDeque<PendingConnectionRead>,
}

enum ConnectionReadAdmission {
    Ready {
        stream: TcpStream,
        permit: PeerReadPermit,
    },
    Refused(TcpStream),
}

impl ConnectionReadBacklog {
    fn defer(&mut self, stream: TcpStream, peer: IpAddr, now: Instant) -> Result<(), TcpStream> {
        if self.pending.len() >= CONNECTION_READ_QUEUE_CAPACITY {
            return Err(stream);
        }
        self.pending.push_back(PendingConnectionRead {
            stream,
            peer,
            expires: now + Duration::from_millis(CONNECTION_READ_WAIT_MILLIS),
        });
        Ok(())
    }

    fn poll(&mut self, peers: &PeerReadAdmission, now: Instant) -> Option<ConnectionReadAdmission> {
        // Rotate blocked peers rather than reserving a reader or blocking another peer's intake.
        // Handle at most one admission/refusal per accept-loop pass, including expired sockets.
        for _ in 0..self.pending.len() {
            let pending = self.pending.pop_front().expect("bounded read backlog");
            if now >= pending.expires {
                return Some(ConnectionReadAdmission::Refused(pending.stream));
            }
            if let Some(permit) = peers.try_acquire(pending.peer) {
                return Some(ConnectionReadAdmission::Ready {
                    stream: pending.stream,
                    permit,
                });
            }
            self.pending.push_back(pending);
        }
        None
    }

    fn pop_stream(&mut self) -> Option<TcpStream> {
        self.pending.pop_front().map(|pending| pending.stream)
    }
}

fn submit_connection_read(
    admission: ConnectionReadAdmission,
    ready: &Sender<ReadyConnection>,
    executor: &BoundedRequestExecutor<ConnectionReadJob>,
    runtime: &DaemonRuntime,
) {
    let job = match admission {
        ConnectionReadAdmission::Ready { stream, permit } => ConnectionReadJob {
            stream,
            ready: ready.clone(),
            _peer_permit: permit,
        },
        ConnectionReadAdmission::Refused(stream) => {
            let (status, body) = daemon_busy_response();
            drain_and_write_refusal(stream, status, &body);
            return;
        }
    };
    match executor.try_submit(job) {
        Ok(()) => record_connection_accepted(runtime),
        Err(SubmitError::Full(job)) => {
            let (status, body) = daemon_busy_response();
            drain_and_write_refusal(job.stream, status, &body);
        }
        Err(SubmitError::Closed(job)) => {
            let (status, body) = daemon_shutting_down_response();
            drain_and_write_refusal(job.stream, status, &body);
        }
    }
}
