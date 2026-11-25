// Transport-only seam: the same media grants, permits, loops and role guards own TCP and LAN TLS.
trait LiveMediaSocket: Read + Write + Send + 'static {
    fn prepare_media(&mut self) -> std::io::Result<()>;
    fn set_media_read_timeout(&mut self, timeout: Duration) -> std::io::Result<()>;
}

impl LiveMediaSocket for TcpStream {
    fn prepare_media(&mut self) -> std::io::Result<()> {
        self.set_read_timeout(Some(LIVE_MEDIA_SOCKET_TIMEOUT))?;
        self.set_write_timeout(Some(LIVE_MEDIA_SOCKET_TIMEOUT))
    }
    fn set_media_read_timeout(&mut self, timeout: Duration) -> std::io::Result<()> {
        self.set_read_timeout(Some(timeout))
    }
}

struct LanMediaStream {
    stream: loom_local_channel::lan_tls::LanStream,
    stopped: Arc<AtomicBool>,
    read_timeout: Duration,
}

impl LanMediaStream {
    fn new(stream: loom_local_channel::lan_tls::LanStream, stopped: Arc<AtomicBool>) -> Self {
        Self {
            stream,
            stopped,
            read_timeout: LIVE_MEDIA_SOCKET_TIMEOUT,
        }
    }
    fn operation(&mut self, timeout: Duration) -> std::io::Result<()> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(std::io::ErrorKind::ConnectionAborted.into());
        }
        self.stream
            .sock
            .set_operation_deadline(Instant::now() + timeout);
        Ok(())
    }
}

impl LiveMediaSocket for LanMediaStream {
    fn prepare_media(&mut self) -> std::io::Result<()> {
        self.stream.conn.set_buffer_limit(Some(32 * 1024));
        self.stream.sock.finish_handshake(LIVE_MEDIA_SOCKET_TIMEOUT)
    }
    fn set_media_read_timeout(&mut self, timeout: Duration) -> std::io::Result<()> {
        self.read_timeout = timeout;
        Ok(())
    }
}

impl Read for LanMediaStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.operation(self.read_timeout)?;
        self.stream.read(buffer)
    }
}
impl Write for LanMediaStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.operation(LIVE_MEDIA_SOCKET_TIMEOUT)?;
        self.stream.write(buffer)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.operation(LIVE_MEDIA_SOCKET_TIMEOUT)?;
        self.stream.flush()
    }
}
