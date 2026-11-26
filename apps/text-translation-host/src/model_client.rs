//! Only the process-scoped loopback broker is exposed, never a provider URL or credential.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::translation_input::TranslationProviderMode;

const MODEL_BROKER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(64);
const MAX_BROKER_FRAME_BYTES: usize = 256 * 1024;

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
    if bytes.len() > MAX_BROKER_FRAME_BYTES {
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
    if length == 0 || length > MAX_BROKER_FRAME_BYTES {
        bail!("translation response exceeds the broker limit");
    }
    let mut frame = vec![0; length];
    read_until(&mut stream, &mut frame, deadline)?;
    match decode_broker_response(&frame)? {
        Ok(text) => Ok(text),
        Err(message) => bail!("{}", message.chars().take(256).collect::<String>()),
    }
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

/// Decodes a single-field broker JSON envelope (`{"text":"..."}` or `{"error":"..."}`)
/// without invoking polymorphic `serde` deserializers on raw socket bytes.
fn decode_broker_response(frame: &[u8]) -> Result<std::result::Result<String, String>> {
    let raw = std::str::from_utf8(frame).context("Loom model broker response is not UTF-8")?;
    let mut rest = raw.trim();
    rest = rest
        .strip_prefix('{')
        .context("Loom model broker returned an invalid envelope")?
        .trim_start();
    let (key, after_key) = parse_json_string(rest)?;
    rest = after_key
        .trim_start()
        .strip_prefix(':')
        .context("Loom model broker envelope is missing field separator")?
        .trim_start();
    let (value, after_val) = parse_json_string(rest)?;
    if after_val.trim_start() != "}" {
        bail!("Loom model broker envelope contained unexpected fields");
    }
    match key.as_str() {
        "text" => Ok(Ok(value)),
        "error" => Ok(Err(value)),
        _ => bail!("Loom model broker returned an unknown envelope key"),
    }
}

fn parse_json_string(input: &str) -> Result<(String, &str)> {
    let body = input
        .strip_prefix('"')
        .context("expected JSON string in broker envelope")?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '"' => return Ok((out, &body[idx + 1..])),
            '\\' => {
                let (_, esc) = chars
                    .next()
                    .context("unterminated escape in broker envelope")?;
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let code = take_hex4(&mut chars)?;
                        let scalar = if (0xD800..=0xDBFF).contains(&code) {
                            let (_, bslash) = chars
                                .next()
                                .context("missing low surrogate in broker envelope")?;
                            let (_, u_ch) = chars
                                .next()
                                .context("missing low surrogate marker in broker envelope")?;
                            if bslash != '\\' || u_ch != 'u' {
                                bail!("invalid surrogate pair in broker envelope");
                            }
                            let low = take_hex4(&mut chars)?;
                            if !(0xDC00..=0xDFFF).contains(&low) {
                                bail!("invalid UTF-16 low surrogate in broker envelope");
                            }
                            0x10000 + (((u32::from(code - 0xD800)) << 10) | u32::from(low - 0xDC00))
                        } else {
                            u32::from(code)
                        };
                        let decoded = char::from_u32(scalar)
                            .context("invalid Unicode scalar in broker envelope")?;
                        out.push(decoded);
                    }
                    _ => bail!("unsupported JSON escape in broker envelope"),
                }
            }
            c if c <= '\u{001f}' => bail!("unescaped control character in broker envelope"),
            c => {
                let decoded =
                    char::from_u32(u32::from(c)).context("invalid scalar in broker envelope")?;
                out.push(decoded);
            }
        }
    }
    bail!("unterminated JSON string in broker envelope")
}

fn take_hex4(chars: &mut std::str::CharIndices<'_>) -> Result<u16> {
    let mut acc: u16 = 0;
    for _ in 0..4 {
        let (_, c) = chars
            .next()
            .context("truncated Unicode escape in broker envelope")?;
        let digit = c
            .to_digit(16)
            .context("invalid hex digit in broker envelope")? as u16;
        acc = (acc << 4) | digit;
    }
    Ok(acc)
}

#[cfg(test)]
mod tests {
    use super::decode_broker_response;

    #[test]
    fn decodes_valid_broker_text_and_error_envelopes_without_serde_deserialization() {
        assert_eq!(
            decode_broker_response(r#"{"text":"安全设置\n\"行二\""}"#.as_bytes())
                .unwrap()
                .unwrap(),
            "安全设置\n\"行二\""
        );
        assert_eq!(
            decode_broker_response(br#"{"text":"\u4f60\u597d \ud83d\ude80"}"#)
                .unwrap()
                .unwrap(),
            "你好 🚀"
        );
        assert_eq!(
            decode_broker_response(br#"{"error":"broker limit"}"#)
                .unwrap()
                .unwrap_err(),
            "broker limit"
        );
        assert!(decode_broker_response(br#"{"text":"a","extra":1}"#).is_err());
        assert!(decode_broker_response(br#"{"unknown":"a"}"#).is_err());
    }

    #[test]
    fn json_control_range_matches_escaped_unicode() {
        for code in 0x7f..=0x9f {
            let ch = char::from_u32(code).unwrap();
            let raw = format!("{{\"text\":\"{ch}\"}}");
            let escaped = format!("{{\"text\":\"\\u{code:04x}\"}}");
            assert_eq!(
                decode_broker_response(raw.as_bytes()).unwrap(),
                decode_broker_response(escaped.as_bytes()).unwrap()
            );
        }
        assert!(decode_broker_response(b"{\"text\":\"\x1f\"}").is_err());
    }
}
