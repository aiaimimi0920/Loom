//! Only the process-scoped loopback broker is exposed, never a provider URL or credential.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::translation_input::TranslationProviderMode;

const MODEL_BROKER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(64);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelResponse {
    text: Option<String>,
    error: Option<String>,
}

pub fn complete(
    system: &str,
    user: &str,
    response_schema: &Value,
    provider_mode: TranslationProviderMode,
) -> Result<String> {
    let address: SocketAddr = std::env::var("LOOM_MODEL_BROKER_ADDRESS")
        .context("Loom model broker is unavailable")?
        .parse()?;
    let token =
        std::env::var("LOOM_MODEL_BROKER_TOKEN").context("Loom model broker is unavailable")?;
    if address.ip() != std::net::Ipv4Addr::LOCALHOST || token.len() != 64 {
        bail!("Loom model broker configuration is invalid");
    }
    let mode = match provider_mode {
        TranslationProviderMode::Auto => "auto",
        TranslationProviderMode::Local => "local",
        TranslationProviderMode::Gateway => "gateway",
    };
    let bytes = serde_json::to_vec(&json!({
        "token": token,
        "system": system,
        "user": user,
        "responseSchema": response_schema,
        "providerMode": mode
    }))?;
    if bytes.len() > 256 * 1024 {
        bail!("translation request exceeds the broker limit");
    }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)?;
    // Auto mode can spend 35 seconds locally and 25 seconds at Gateway, plus relay overhead.
    let deadline = Instant::now() + MODEL_BROKER_RESPONSE_TIMEOUT;
    let mut header = [0; 4];
    read_until(&mut stream, &mut header, deadline)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > 256 * 1024 {
        bail!("translation response exceeds the broker limit");
    }
    let mut bytes = vec![0; length];
    read_until(&mut stream, &mut bytes, deadline)?;
    let response: ModelResponse = serde_json::from_slice(&bytes)?;
    if let Some(message) = response.error {
        bail!("{}", message.chars().take(256).collect::<String>());
    }
    response.text.context("Loom model broker returned no text")
}

fn read_until(stream: &mut TcpStream, mut bytes: &mut [u8], deadline: Instant) -> Result<()> {
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("translation timed out")?;
        stream.set_read_timeout(Some(remaining))?;
        let count = stream.read(bytes)?;
        if count == 0 {
            bail!("Loom model broker closed the response");
        }
        bytes = &mut bytes[count..];
    }
    Ok(())
}
