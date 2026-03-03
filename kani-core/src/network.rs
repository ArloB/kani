//! Network-related security utilities.

use std::net::IpAddr;

/// Returns `true` if the given URL targets a private, loopback, link-local,
/// or otherwise reserved address that should not be reachable from a proxy or
/// sandboxed HTTP client.
pub fn is_private_host(url: &str) -> bool {
    let Ok(parsed) = url.parse::<rquest::Url>() else {
        return true;
    };

    if let Some(host) = parsed.host_str()
        && (host == "localhost" || host.ends_with(".local"))
    {
        return true;
    }

    let ip = match parsed.host() {
        Some(host) => match host.to_string().parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => return false,
        },
        None => return true,
    };

    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()
                || ipv4.is_private()
                || ipv4.is_link_local()
                || ipv4.octets() == [169, 254, 169, 254]
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
                || (ipv6.segments()[0] & 0xfe00) == 0xfc00
                || (ipv6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}
