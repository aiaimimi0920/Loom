//! One process-scoped, bounded loopback model broker. No general URL or credential API.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use loom_process::ProcessSpec;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

const MAX_FRAME: usize = 256 * 1024;
const MAX_RESPONSE: usize = 128 * 1024;
const MAX_REQUESTS: usize = 128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelRequest {
    token: String,
    system: String,
    user: String,
    #[serde(default, rename = "responseSchema")]
    response_schema: Option<Value>,
    #[serde(default, rename = "providerMode")]
    provider_mode: Option<String>,
}

pub(crate) struct ModelBroker {
    address: String,
    token: String,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl ModelBroker {
    pub(crate) fn start() -> std::io::Result<Self> {
        Self::with_mode_handler(super::model_provider::complete)
    }

    #[cfg(test)]
    fn with_handler(
        complete: impl Fn(String, String) -> Result<String, String> + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::with_mode_handler(move |system, user, _schema, _provider_mode| complete(system, user))
    }

    fn with_mode_handler(
        complete: impl Fn(String, String, Option<Value>, Option<String>) -> Result<String, String>
            + Send
            + 'static,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?.to_string();
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let secret = token.clone();
        let (stop, stopped) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("loom-model-broker".to_owned())
            .spawn(move || {
                let mut requests = 0;
                loop {
                    if stopped.try_recv().is_ok() {
                        break;
                    }
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let Ok(request) = read_request(&mut stream) else {
                                continue;
                            };
                            let authorized = request
                                .token
                                .bytes()
                                .zip(secret.bytes())
                                .fold(0u8, |difference, (left, right)| difference | (left ^ right))
                                == 0;
                            let response = if !authorized {
                                json!({ "error": "model broker authorization failed" })
                            } else if !super::model_schema::validate(
                                request.response_schema.as_ref(),
                            ) {
                                json!({ "error": "model broker response schema is invalid" })
                            } else if let Err(message) =
                                validate_provider_mode(request.provider_mode.as_deref())
                            {
                                json!({ "error": message })
                            } else if requests >= MAX_REQUESTS {
                                json!({ "error": "model broker request limit reached" })
                            } else {
                                requests += 1;
                                match complete(
                                    request.system,
                                    request.user,
                                    request.response_schema,
                                    request.provider_mode,
                                ) {
                                    Ok(text)
                                        if !text.trim().is_empty()
                                            && text.len() <= MAX_RESPONSE =>
                                    {
                                        json!({ "text": text })
                                    }
                                    Ok(_) => {
                                        json!({ "error": "model response is empty or too large" })
                                    }
                                    Err(message) => json!({ "error": message }),
                                }
                            };
                            if let Ok(bytes) = serde_json::to_vec(&response) {
                                let deadline = Instant::now() + Duration::from_secs(3);
                                let _ = write_until(
                                    &mut stream,
                                    &(bytes.len() as u32).to_be_bytes(),
                                    deadline,
                                )
                                .and_then(|_| write_until(&mut stream, &bytes, deadline));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if stopped.recv_timeout(Duration::from_millis(20)).is_ok() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            address,
            token,
            stop,
            worker: Some(worker),
        })
    }

    pub(crate) fn configure(&self, spec: &mut ProcessSpec) {
        spec.env
            .insert("LOOM_MODEL_BROKER_ADDRESS".to_owned(), self.address.clone());
        spec.env
            .insert("LOOM_MODEL_BROKER_TOKEN".to_owned(), self.token.clone());
    }
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<ModelRequest> {
    // One deadline covers the entire frame, including clients that trickle bytes.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut header = [0; 4];
    read_until(stream, &mut header, deadline)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0; length];
    read_until(stream, &mut bytes, deadline)?;
    let request: ModelRequest = serde_json::from_slice(&bytes)?;
    if request.token.len() != 64 || request.system.len() > 8192 || request.user.len() > 192 * 1024 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    Ok(request)
}

fn validate_provider_mode(mode: Option<&str>) -> Result<(), String> {
    if mode.is_some_and(|value| value.len() > 16 || !matches!(value, "auto" | "local" | "gateway"))
    {
        return Err("model broker provider mode is invalid".to_owned());
    }
    Ok(())
}

fn remaining(deadline: Instant) -> std::io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| std::io::ErrorKind::TimedOut.into())
}

fn read_until(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> std::io::Result<()> {
    while !bytes.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let count = stream.read(bytes)?;
        if count == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes = &mut bytes[count..];
    }
    Ok(())
}

fn write_until(stream: &mut TcpStream, mut bytes: &[u8], deadline: Instant) -> std::io::Result<()> {
    while !bytes.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        let count = stream.write(bytes)?;
        if count == 0 {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        bytes = &bytes[count..];
    }
    Ok(())
}

impl Drop for ModelBroker {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(address: &str, token: &str) -> serde_json::Value {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let bytes = serde_json::to_vec(&json!({
            "token": token,
            "system": "system",
            "user": "words"
        }))
        .unwrap();
        stream
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&bytes).unwrap();
        let mut header = [0; 4];
        stream.read_exact(&mut header).unwrap();
        let mut response = vec![0; u32::from_be_bytes(header) as usize];
        stream.read_exact(&mut response).unwrap();
        serde_json::from_slice(&response).unwrap()
    }
    #[test]
    fn scoped_token_and_listener_lifetime_are_enforced() {
        let broker = ModelBroker::with_handler(|_, user| Ok(format!("translated {user}"))).unwrap();
        assert!(call(&broker.address, &"x".repeat(64))["error"].is_string());
        assert_eq!(
            call(&broker.address, &broker.token)["text"],
            "translated words"
        );
        let address = broker.address.clone();
        drop(broker);
        assert!(TcpStream::connect(address).is_err());
    }

    #[test]
    fn provider_mode_is_forwarded_and_unknown_modes_are_rejected() {
        let broker = ModelBroker::with_mode_handler(|_, _, schema, mode| {
            assert_eq!(schema, Some(json!({ "type": "string" })));
            Ok(mode.unwrap_or_else(|| "legacy".to_owned()))
        })
        .unwrap();
        let mut stream = TcpStream::connect(&broker.address).unwrap();
        let request = serde_json::to_vec(&json!({
            "token": broker.token.clone(),
            "system": "system",
            "user": "words",
            "responseSchema": { "type": "string" },
            "providerMode": "local"
        }))
        .unwrap();
        stream
            .write_all(&(request.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&request).unwrap();
        let mut header = [0; 4];
        stream.read_exact(&mut header).unwrap();
        let mut response = vec![0; u32::from_be_bytes(header) as usize];
        stream.read_exact(&mut response).unwrap();
        let response: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(response["text"], "local");

        let mut stream = TcpStream::connect(&broker.address).unwrap();
        let request = serde_json::to_vec(&json!({
            "token": broker.token.clone(),
            "system": "system",
            "user": "words",
            "providerMode": "other"
        }))
        .unwrap();
        stream
            .write_all(&(request.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&request).unwrap();
        let mut header = [0; 4];
        stream.read_exact(&mut header).unwrap();
        let mut response = vec![0; u32::from_be_bytes(header) as usize];
        stream.read_exact(&mut response).unwrap();
        let response: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(response["error"], "model broker provider mode is invalid");
    }
    #[test]
    fn oversized_frame_is_rejected_before_allocation() {
        let broker = ModelBroker::with_handler(|_, _| panic!("must not invoke Gateway")).unwrap();
        let mut stream = TcpStream::connect(&broker.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .write_all(&((MAX_FRAME + 1) as u32).to_be_bytes())
            .unwrap();
        let mut byte = [0];
        assert!(!matches!(stream.read(&mut byte), Ok(1)));
    }
    #[test]
    fn trickled_frame_cannot_extend_the_absolute_deadline() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let writer = std::thread::spawn(move || {
            for _ in 0..20 {
                if client.write_all(&[1]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let start = Instant::now();
        let error = read_until(
            &mut server,
            &mut [0; 100],
            start + Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
        drop(server);
        writer.join().unwrap();
    }
}
