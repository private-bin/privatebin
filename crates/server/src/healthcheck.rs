//! `privatebin healthcheck`: probes `/healthz` over plain TCP so it works in
//! a distroless image without curl.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Returns whether the server listening on `bind` answers `/healthz` with 200.
pub fn check(bind: SocketAddr) -> Result<(), String> {
    let ip = match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    let addr = SocketAddr::new(ip, bind.port());
    let mut stream =
        TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|e| format!("connect {addr}: {e}"))?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .map_err(|e| format!("write: {e}"))?;
    let mut response = Vec::new();
    stream
        .take(4096)
        .read_to_end(&mut response)
        .map_err(|e| format!("read: {e}"))?;
    let status_line = response.split(|&b| b == b'\n').next().unwrap_or_default();
    let status = String::from_utf8_lossy(status_line);
    match status.split_whitespace().nth(1) {
        Some("200") => Ok(()),
        _ => Err(format!("unhealthy: {}", status.trim())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn healthy_and_unhealthy() {
        for (reply, ok) in [
            ("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok", true),
            ("HTTP/1.1 503 Service Unavailable\r\n\r\n", false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf).unwrap();
                s.write_all(reply.as_bytes()).unwrap();
            });
            assert_eq!(check(addr).is_ok(), ok);
            server.join().unwrap();
        }
    }

    #[test]
    fn connection_refused_is_unhealthy() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        assert!(check(addr).is_err());
    }
}
