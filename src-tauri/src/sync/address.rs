use crate::sync::{SyncError, SyncResult, DEFAULT_PORT};
use std::net::{SocketAddr, UdpSocket};

pub fn normalize_address(address: &str) -> SyncResult<String> {
    normalize_address_with_port(address, DEFAULT_PORT)
}

pub fn normalize_address_with_port(address: &str, default_port: u16) -> SyncResult<String> {
    let trimmed = address.trim();
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let without_path = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .trim();
    if without_path.is_empty() {
        return Err(SyncError::Address("empty address".into()));
    }
    if without_path.starts_with('[') {
        if without_path.contains("]:") {
            return Ok(without_path.to_string());
        }
        return Ok(format!("{without_path}:{default_port}"));
    }
    if without_path.matches(':').count() == 1 {
        Ok(without_path.to_string())
    } else if without_path.matches(':').count() > 1 {
        Ok(format!("[{without_path}]:{default_port}"))
    } else {
        Ok(format!("{without_path}:{default_port}"))
    }
}

pub fn best_effort_listen_address(bound: Option<SocketAddr>) -> String {
    let port = bound.map(|addr| addr.port()).unwrap_or(DEFAULT_PORT);
    let ip = UdpSocket::bind("0.0.0.0:0")
        .and_then(|sock| {
            let _ = sock.connect("8.8.8.8:80");
            sock.local_addr()
        })
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "0.0.0.0".into());
    format!("{ip}:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_addresses() {
        assert_eq!(normalize_address("100.1.2.3").unwrap(), "100.1.2.3:47821");
        assert_eq!(
            normalize_address("http://host:12/v1/hello").unwrap(),
            "host:12"
        );
        assert_eq!(normalize_address("host.local").unwrap(), "host.local:47821");
        assert_eq!(
            normalize_address("2001:db8::1").unwrap(),
            "[2001:db8::1]:47821"
        );
        assert_eq!(
            normalize_address("[2001:db8::1]:99").unwrap(),
            "[2001:db8::1]:99"
        );
    }
}
