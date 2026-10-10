use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// A single absolute deadline bounds slow-trickle TLS and HTTP upgrade handshakes.
pub struct DeadlineStream {
    stream: TcpStream,
    deadline: Option<Instant>,
}

impl DeadlineStream {
    pub fn new(stream: TcpStream, deadline: Instant) -> io::Result<Self> {
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            deadline: Some(deadline),
        })
    }

    pub fn finish_handshake(&mut self, timeout: Duration) -> io::Result<()> {
        self.stream.set_read_timeout(Some(timeout))?;
        self.stream.set_write_timeout(Some(timeout))?;
        self.deadline = None;
        Ok(())
    }

    /// Bound one application read/write operation, including slow-trickle I/O.
    pub fn set_operation_deadline(&mut self, deadline: Instant) {
        self.deadline = Some(deadline);
    }

    /// The owner uses shutdown on this handle to interrupt a blocking worker.
    pub fn interrupt_handle(&self) -> io::Result<TcpStream> {
        self.stream.try_clone()
    }

    fn prepare(&self) -> io::Result<()> {
        if let Some(deadline) = self.deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.stream.set_read_timeout(Some(remaining))?;
            self.stream.set_write_timeout(Some(remaining))?;
        }
        Ok(())
    }
}

impl Read for DeadlineStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.prepare()?;
        self.stream.read(buffer)
    }
}

impl Write for DeadlineStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.prepare()?;
        self.stream.write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.prepare()?;
        self.stream.flush()
    }
}
