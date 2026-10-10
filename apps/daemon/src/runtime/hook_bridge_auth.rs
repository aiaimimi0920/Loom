// Authenticates the transport before either Hook or extension messages can dispatch.
fn accept_authenticated_hook_socket(
    stream: TcpStream,
    token: &str,
) -> std::result::Result<tungstenite::WebSocket<TcpStream>, ()> {
    tungstenite::accept_hdr(
        stream,
        |request: &tungstenite::handshake::server::Request,
         mut response: tungstenite::handshake::server::Response| {
            match authorize_hook_upgrade(request, token) {
                Ok(browser_protocol) => {
                    if browser_protocol {
                        response
                            .headers_mut()
                            .insert("sec-websocket-protocol", "loom.hook.v1".parse().unwrap());
                    }
                    Ok(response)
                }
                Err(status) => Err(tungstenite::http::Response::builder()
                    .status(status)
                    .body(Some("Hook transport authorization denied".to_owned()))
                    .unwrap()),
            }
        },
    )
    .map_err(|_| ())
}

fn authorize_hook_upgrade(
    request: &tungstenite::handshake::server::Request,
    expected: &str,
) -> std::result::Result<bool, u16> {
    let headers = request.headers();
    if expected.is_empty() || request.uri().path() != "/" || request.uri().query().is_some() {
        return Err(401);
    }
    let one = |name: &str| -> Option<&str> {
        let mut values = headers.get_all(name).iter();
        let first = values.next()?.to_str().ok()?;
        values.next().is_none().then_some(first)
    };
    if one("host").and_then(loopback_host_from_authority).is_none() {
        return Err(403);
    }
    if headers.contains_key("origin")
        && !one("origin").is_some_and(|origin| {
            matches!(
                origin,
                "tauri://localhost"
                    | "http://tauri.localhost"
                    | "https://tauri.localhost"
                    | "http://localhost:1420"
                    | "http://127.0.0.1:1420"
                    | "http://localhost:1423"
                    | "http://127.0.0.1:1423"
            )
        })
    {
        return Err(403);
    }
    let (provided, browser_protocol) = if headers.contains_key("authorization") {
        if headers.contains_key("sec-websocket-protocol") {
            return Err(401);
        }
        let mut credentials = headers.get_all("authorization").iter();
        let value = credentials.next().ok_or(401u16)?.as_bytes();
        if credentials.next().is_some() {
            return Err(401);
        }
        (
            value.strip_prefix(b"Bearer ").ok_or(401u16)?.to_vec(),
            false,
        )
    } else {
        // Browser WebSocket cannot set Authorization. Encode the credential in
        // an offered protocol, never a URL; the response echoes only the public protocol.
        let offered = one("sec-websocket-protocol").ok_or(401u16)?;
        if offered.len() > 5600 {
            return Err(401);
        }
        let mut protocols = offered.split(',').map(str::trim);
        if protocols.next() != Some("loom.hook.v1") {
            return Err(401);
        }
        let encoded = protocols
            .next()
            .and_then(|value| value.strip_prefix("loom.auth."))
            .ok_or(401u16)?;
        if protocols.next().is_some() {
            return Err(401);
        }
        (BASE64_URL.decode(encoded).map_err(|_| 401u16)?, true)
    };
    if provided.len() != expected.len()
        || provided.is_empty()
        || provided
            .iter()
            .zip(expected.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            != 0
    {
        return Err(401);
    }
    Ok(browser_protocol)
}
