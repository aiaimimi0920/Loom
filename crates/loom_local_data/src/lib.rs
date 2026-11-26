//! Shared client/desktop connection profile (`local` bundled core vs `remote` LAN/TLS server)
//! and portable `.loom` data root resolution, modeled on Neuro's dual-layer architecture.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
mod connection_token;

const CONNECTION_FILE_NAME: &str = "connection.json";
const REMOTE_TOKEN_FILE_NAME: &str = "connection-remote-token";
const MAX_CONNECTION_FILE_BYTES: usize = 64 * 1024;
const MAX_TOKEN_BYTES: usize = 4096;

/// Operating mode of a Loom desktop/client shell.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    /// Start and manage the bundled local `loom-daemon` on loopback.
    #[default]
    Local,
    /// Connect to an external Loom server over LAN or HTTPS without starting a local daemon.
    Remote,
}

impl ConnectionMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}

/// Persisted non-secret connection profile stored in `<control-plane>/connection.json`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomConnectionProfile {
    #[serde(default)]
    pub mode: ConnectionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otter_bridge_url: Option<String>,
}

/// Safe public view returned to the UI (never exposes the bearer token).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomConnectionStatus {
    pub mode: ConnectionMode,
    pub active_url: String,
    pub remote_url: Option<String>,
    pub server_label: Option<String>,
    pub otter_bridge_url: Option<String>,
    pub has_remote_token: bool,
}

/// Update payload sent from the desktop UI when switching between `local` and `remote` server modes.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomConnectionUpdate {
    pub mode: ConnectionMode,
    #[serde(default)]
    pub remote_url: Option<String>,
    #[serde(default)]
    pub server_label: Option<String>,
    #[serde(default)]
    pub otter_bridge_url: Option<String>,
    #[serde(default)]
    pub remote_token: Option<String>,
    #[serde(default)]
    pub clear_remote_token: bool,
}

/// Parsed HTTP endpoint components for desktop-to-daemon requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedLoomEndpoint {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub is_loopback: bool,
    pub is_private_lan: bool,
}

impl ParsedLoomEndpoint {
    #[must_use]
    pub fn authority(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    #[must_use]
    pub fn origin(&self) -> String {
        format!("{}://{}", self.scheme, self.authority())
    }

    pub fn resolve_socket_addr(&self) -> Result<SocketAddr, String> {
        let host = self.host.trim_matches(['[', ']']);
        if host.eq_ignore_ascii_case("localhost") {
            return Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port));
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(SocketAddr::new(ip, self.port));
        }
        (host, self.port)
            .to_socket_addrs()
            .map_err(|error| {
                format!(
                    "Failed to resolve Loom server address `{host}:{}`: {error}",
                    self.port
                )
            })?
            .next()
            .ok_or_else(|| format!("Could not find Loom server address `{host}:{}`", self.port))
    }
}

fn is_private_lan_ip(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.is_private(),
        IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

/// Validate a Loom daemon URL according to `allow_remote`:
/// - `allow_remote == false`: strictly `http://127.0.0.1:<port>`, `http://localhost:<port>`, or `http://[::1]:<port>`.
/// - `allow_remote == true`: allows loopback HTTP/HTTPS, private RFC1918/ULA LAN HTTP/HTTPS, or remote HTTPS.
pub fn parse_loom_endpoint_url(
    raw_url: &str,
    allow_remote: bool,
) -> Result<ParsedLoomEndpoint, String> {
    let trimmed = raw_url.trim().trim_end_matches('/');
    let (scheme, rest) = if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http", rest)
    } else if let Some(rest) = trimmed.strip_prefix("https://") {
        if !allow_remote {
            return Err("Loom local service URL must use an http:// loopback address.".to_owned());
        }
        ("https", rest)
    } else {
        return Err("Loom server URL must start with http:// or https://.".to_owned());
    };

    if rest.is_empty()
        || rest.contains('/')
        || rest.contains('?')
        || rest.contains('#')
        || rest.contains('@')
        || rest.contains('\\')
        || rest.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err("Loom server URL may contain only a host and port, without paths, query parameters, or credentials.".to_owned());
    }

    let (host_raw, port_raw) = if let Some(bracketed) = rest.strip_prefix('[') {
        let end = bracketed
            .find(']')
            .ok_or_else(|| "IPv6 host address is missing its closing bracket `]`.".to_owned())?;
        let host = &bracketed[..end];
        if host.parse::<Ipv6Addr>().is_err() {
            return Err("Brackets in a Loom server URL require an IPv6 address.".to_owned());
        }
        let remainder = &bracketed[end + 1..];
        let port = if let Some(port) = remainder.strip_prefix(':') {
            port
        } else if remainder.is_empty() && scheme == "https" && allow_remote {
            "443"
        } else {
            return Err("Loom server URL must include an explicit port.".to_owned());
        };
        (host, port)
    } else if rest.contains(['[', ']']) || rest.bytes().filter(|&b| b == b':').count() > 1 {
        return Err("Use [address]:port for IPv6 Loom server URLs.".to_owned());
    } else if let Some((host, port)) = rest.rsplit_once(':') {
        (host, port)
    } else if scheme == "https" && allow_remote {
        (rest, "443")
    } else {
        return Err("Loom server URL must include a port.".to_owned());
    };

    if host_raw.is_empty() {
        return Err("Loom server host cannot be empty.".to_owned());
    }
    let port = port_raw
        .parse::<u16>()
        .map_err(|error| format!("Invalid Loom server port: {error}"))?;
    if port == 0 {
        return Err("Loom server port cannot be 0.".to_owned());
    }

    let parsed_ip = host_raw.parse::<IpAddr>().ok().map(|ip| ip.to_canonical());
    let is_loopback = host_raw.eq_ignore_ascii_case("localhost")
        || parsed_ip.is_some_and(|ip| ip.is_loopback())
        || host_raw == "::1";
    let is_private_lan = parsed_ip.is_some_and(is_private_lan_ip);

    if !allow_remote && !is_loopback {
        return Err(
            "Loom desktop connects only to local services on loopback addresses.".to_owned(),
        );
    }
    if scheme == "http" && !is_loopback && !is_private_lan {
        return Err(
            "Remote public or domain Loom servers must use https://; plaintext http:// is allowed only for loopback or private LAN IPs."
                .to_owned(),
        );
    }

    Ok(ParsedLoomEndpoint {
        scheme: scheme.to_owned(),
        host: if host_raw == "::1" {
            "::1".to_owned()
        } else {
            host_raw.to_owned()
        },
        port,
        is_loopback,
        is_private_lan,
    })
}

/// Resolve an optional executable-adjacent `.loom` portable data root if present.
pub fn resolve_portable_root(executable: &Path) -> io::Result<Option<PathBuf>> {
    let Some(parent) = executable.parent() else {
        return Ok(None);
    };
    let candidate = parent.join(".loom");
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::other(
                    "Executable-adjacent .loom must be a regular directory",
                ));
            }
            Ok(Some(candidate))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Manages `<control-plane>/connection.json` and `<control-plane>/connection-remote-token`.
#[derive(Clone, Debug)]
pub struct LoomConnectionStore {
    root: PathBuf,
}

impl LoomConnectionStore {
    #[must_use]
    pub fn new(control_plane_root: impl Into<PathBuf>) -> Self {
        Self {
            root: control_plane_root.into(),
        }
    }

    fn profile_path(&self) -> PathBuf {
        self.root.join(CONNECTION_FILE_NAME)
    }

    fn remote_token_path(&self) -> PathBuf {
        self.root.join(REMOTE_TOKEN_FILE_NAME)
    }

    pub fn load_profile(&self) -> Result<LoomConnectionProfile, String> {
        let path = self.profile_path();
        match read_bounded(&path, MAX_CONNECTION_FILE_BYTES) {
            Ok(Some(bytes)) => serde_json::from_slice(&bytes).map_err(|error| {
                format!(
                    "Failed to parse Loom connection profile `{}`: {error}",
                    path.display()
                )
            }),
            Ok(None) => Ok(LoomConnectionProfile::default()),
            Err(error) => Err(format!(
                "Failed to read Loom connection profile `{}`: {error}",
                path.display()
            )),
        }
    }

    pub fn load_remote_token(&self) -> Result<Option<String>, String> {
        let profile = self.load_profile()?;
        match profile.remote_url {
            Some(url) => self.read_token_for_origin(&parse_loom_endpoint_url(&url, true)?.origin()),
            None => Ok(None),
        }
    }

    /// Bind a request's immutable destination to its credential, even during profile replacement.
    pub fn load_remote_token_for(&self, base_url: &str) -> Result<Option<String>, String> {
        let origin = parse_loom_endpoint_url(base_url, true)?.origin();
        let profile = self.load_profile()?;
        let configured = profile
            .remote_url
            .as_deref()
            .map(|url| parse_loom_endpoint_url(url, true).map(|endpoint| endpoint.origin()))
            .transpose()?;
        if profile.mode != ConnectionMode::Remote || configured.as_deref() != Some(&origin) {
            return Err("Loom connection changed; reload before sending this request.".to_owned());
        }
        self.read_token_for_origin(&origin)
    }

    pub fn status(&self, default_local_url: &str) -> Result<LoomConnectionStatus, String> {
        let profile = self.load_profile()?;
        let has_remote_token = self.load_remote_token()?.is_some();
        let active_url = match profile.mode {
            ConnectionMode::Local => default_local_url.trim().trim_end_matches('/').to_owned(),
            ConnectionMode::Remote => profile
                .remote_url
                .clone()
                .unwrap_or_else(|| default_local_url.trim().trim_end_matches('/').to_owned()),
        };
        Ok(LoomConnectionStatus {
            mode: profile.mode,
            active_url,
            remote_url: profile.remote_url,
            server_label: profile.server_label,
            otter_bridge_url: profile.otter_bridge_url,
            has_remote_token,
        })
    }

    pub fn save_update(
        &self,
        update: LoomConnectionUpdate,
        default_local_url: &str,
    ) -> Result<LoomConnectionStatus, String> {
        let _guard = connection_token::UPDATE_LOCK
            .lock()
            .map_err(|_| "Loom connection update lock is poisoned.".to_owned())?;
        let normalized_remote_url = update
            .remote_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| parse_loom_endpoint_url(value, true).map(|ep| ep.origin()))
            .transpose()?;
        if update.mode == ConnectionMode::Remote && normalized_remote_url.is_none() {
            return Err(
                "A valid Loom server URL is required when switching to remote server mode."
                    .to_owned(),
            );
        }

        let profile = LoomConnectionProfile {
            mode: update.mode,
            remote_url: normalized_remote_url,
            server_label: update
                .server_label
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            otter_bridge_url: update
                .otter_bridge_url
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        };
        let mut encoded = serde_json::to_vec_pretty(&profile)
            .map_err(|error| format!("Failed to serialize Loom connection profile: {error}"))?;
        encoded.push(b'\n');
        if encoded.len() > MAX_CONNECTION_FILE_BYTES {
            return Err("Loom connection profile exceeds the file size limit.".to_owned());
        }
        self.save_bound_token(
            profile.remote_url.as_deref(),
            update.remote_token.as_deref(),
            update.clear_remote_token,
        )?;
        write_private_atomic(&self.profile_path(), &encoded)
            .map_err(|error| format!("Failed to save Loom connection profile: {error}"))?;
        self.status(default_local_url)
    }
}

fn validate_token(token: &str) -> Result<(), String> {
    if token.is_empty()
        || token.len() > MAX_TOKEN_BYTES
        || token.bytes().any(|byte| byte <= b' ' || byte == 0x7f)
    {
        return Err("Invalid Loom authentication token format.".to_owned());
    }
    Ok(())
}

fn read_bounded(path: &Path, max_bytes: usize) -> io::Result<Option<Vec<u8>>> {
    let file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::other("refusing non-regular file"));
    }
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::other("file exceeds maximum byte limit"));
    }
    Ok(Some(bytes))
}

fn write_private_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parser_enforces_local_vs_remote_lan_and_https_rules() {
        let local = parse_loom_endpoint_url("http://127.0.0.1:8765/", false).expect("loopback");
        assert!(local.is_loopback);
        assert_eq!(local.origin(), "http://127.0.0.1:8765");
        assert_eq!(
            local.resolve_socket_addr().expect("addr"),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8765)
        );
        assert!(parse_loom_endpoint_url("http://[::1]:8765", false).is_ok());
        assert_eq!(
            parse_loom_endpoint_url("http://[::1]:8765", false)
                .expect("ipv6 loopback")
                .resolve_socket_addr()
                .expect("ipv6 addr"),
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 8765)
        );

        assert!(parse_loom_endpoint_url("http://192.168.1.50:8765", false).is_err());
        let lan = parse_loom_endpoint_url("http://192.168.1.50:8765", true).expect("lan remote");
        assert!(lan.is_private_lan);
        assert_eq!(lan.origin(), "http://192.168.1.50:8765");

        assert!(parse_loom_endpoint_url("http://8.8.8.8:8765", true).is_err());
        assert!(parse_loom_endpoint_url("http://loom.example.com:8765", true).is_err());
        let tls = parse_loom_endpoint_url("https://loom.example.com:8443", true).expect("https");
        assert_eq!(tls.origin(), "https://loom.example.com:8443");
        for invalid in [
            "https://[::1]suffix",
            "http://[localhost]:8765",
            "http://::1:8765",
            "https://bad host",
        ] {
            assert!(parse_loom_endpoint_url(invalid, true).is_err(), "{invalid}");
        }
    }

    #[test]
    fn connection_store_persists_profile_and_isolates_remote_token() {
        let root = std::env::temp_dir().join(format!(
            "loom-local-data-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("epoch")
                .as_nanos()
        ));
        let store = LoomConnectionStore::new(&root);
        let default_status = store.status("http://127.0.0.1:8765").expect("default");
        assert_eq!(default_status.mode, ConnectionMode::Local);
        assert!(!default_status.has_remote_token);

        let updated = store
            .save_update(
                LoomConnectionUpdate {
                    mode: ConnectionMode::Remote,
                    remote_url: Some("http://192.168.1.88:8765/".to_owned()),
                    server_label: Some("Home NAS".to_owned()),
                    otter_bridge_url: None,
                    remote_token: Some("secret-nas-token".to_owned()),
                    clear_remote_token: false,
                },
                "http://127.0.0.1:8765",
            )
            .expect("save remote");
        assert_eq!(updated.mode, ConnectionMode::Remote);
        assert_eq!(updated.active_url, "http://192.168.1.88:8765");
        assert!(updated.has_remote_token);
        let raw_json = fs::read_to_string(root.join("connection.json")).expect("read json");
        assert!(!raw_json.contains("secret-nas-token"));
        assert_eq!(
            store.load_remote_token().expect("token").as_deref(),
            Some("secret-nas-token")
        );
        let _ = fs::remove_dir_all(root);
    }
}
