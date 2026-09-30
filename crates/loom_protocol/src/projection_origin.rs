//! An invitation advertises one bounded origin, never a URL to an arbitrary resource.
use std::net::IpAddr;

pub fn projection_origin_valid(value: &str) -> bool {
    if value.len() > 256 || !value.is_ascii() {
        return false;
    }
    let Some((scheme, authority)) = value.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "https" | "http") || authority.contains(['/', '\\', '?', '#', '@', '%']) {
        return false;
    }
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let Some((host, suffix)) = rest.split_once(']') else {
            return false;
        };
        if !matches!(host.parse::<IpAddr>(), Ok(IpAddr::V6(_))) {
            return false;
        }
        (host, suffix)
    } else {
        match authority.split_once(':') {
            Some((host, _)) => (host, &authority[host.len()..]),
            None => (authority, ""),
        }
    };
    if !port.is_empty()
        && !port.strip_prefix(':').is_some_and(|number| {
            !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
                && number.parse::<u16>().is_ok_and(|port| port != 0)
        })
    {
        return false;
    }
    let ip = host.parse::<IpAddr>().ok();
    if ip.is_none()
        && (host.is_empty()
            || host.len() > 253
            || !host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            }))
    {
        return false;
    }
    scheme == "https"
        || host.eq_ignore_ascii_case("localhost")
        || ip.is_some_and(|ip| ip.is_loopback())
}
