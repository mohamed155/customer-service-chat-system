//! `server healthcheck`: the container health check (MOH-81).
//!
//! The backend image is `debian:bookworm-slim` and has no `curl`, so it carries its own probe: the
//! same binary asks its own `/ready` endpoint and exits 0 when the answer is `200`, 1 otherwise.
//! Docker and Compose run it as `server healthcheck`, so a container is "healthy" only while
//! PostgreSQL, Redis and object storage answer and the process is not shutting down, not merely
//! while it is running.
//!
//! It reads only `PORT` and `BIND_ADDRESS` (the listener's own settings) and none of the secrets,
//! so it works with the environment the container already has. It makes one plain HTTP/1.1 request
//! over a TCP socket with short timeouts and needs no HTTP client.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// The report is a few hundred bytes; anything much bigger is not our server.
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// `host:port` the probe connects to. A listener bound to every interface (`0.0.0.0`, `::`) is
/// reached on the loopback address.
pub fn probe_address(port: Option<&str>, bind_address: Option<&str>) -> Result<String, String> {
    let port = match port.map(str::trim).filter(|p| !p.is_empty()) {
        None => 8080,
        Some(p) => p
            .parse::<u16>()
            .map_err(|_| format!("PORT must be a port number, got '{p}'"))?,
    };
    let host = match bind_address.map(str::trim) {
        None | Some("") | Some("0.0.0.0") | Some("::") | Some("[::]") => "127.0.0.1".to_owned(),
        Some(h) if h.contains(':') && !h.starts_with('[') => format!("[{h}]"),
        Some(h) => h.to_owned(),
    };
    Ok(format!("{host}:{port}"))
}

/// Ask `address` for `/ready`. `Ok` only for a `200`.
pub fn probe(address: &str) -> Result<(), String> {
    let target: SocketAddr = address
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {address}: {e}"))?
        .next()
        .ok_or_else(|| format!("no address for {address}"))?;
    let mut stream = TcpStream::connect_timeout(&target, CONNECT_TIMEOUT)
        .map_err(|e| format!("cannot connect to {address}: {e}"))?;
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .and_then(|_| stream.set_write_timeout(Some(READ_TIMEOUT)))
        .map_err(|e| format!("cannot set timeouts: {e}"))?;
    stream
        .write_all(b"GET /ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .map_err(|e| format!("cannot send the request: {e}"))?;

    let mut response = Vec::new();
    stream
        .take(MAX_RESPONSE_BYTES)
        .read_to_end(&mut response)
        .map_err(|e| format!("no answer from {address}: {e}"))?;
    let text = String::from_utf8_lossy(&response);
    let status_line = text.lines().next().unwrap_or("");
    if status_line.split_whitespace().nth(1) == Some("200") {
        Ok(())
    } else {
        Err(format!("{address}/ready answered '{status_line}'"))
    }
}

/// Entry point of `server healthcheck`: the process exit code.
pub fn run() -> i32 {
    let address = match probe_address(
        std::env::var("PORT").ok().as_deref(),
        std::env::var("BIND_ADDRESS").ok().as_deref(),
    ) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("healthcheck: {e}");
            return 1;
        }
    };
    match probe(&address) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("healthcheck: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A one-shot server that answers the first request with `reply` (nothing if `None`).
    fn serve_once(reply: Option<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 1024];
            let n = socket.read(&mut buf).unwrap_or(0);
            assert!(
                String::from_utf8_lossy(&buf[..n]).starts_with("GET /ready HTTP/1.1"),
                "the probe asks for /ready"
            );
            if let Some(reply) = reply {
                let _ = socket.write_all(reply.as_bytes());
            }
        });
        address
    }

    #[test]
    fn a_200_is_healthy() {
        let address = serve_once(Some("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}"));
        assert_eq!(probe(&address), Ok(()));
    }

    #[test]
    fn a_503_is_not_healthy_and_says_so() {
        let address = serve_once(Some("HTTP/1.1 503 Service Unavailable\r\n\r\n{}"));
        let err = probe(&address).unwrap_err();
        assert!(err.contains("503"), "{err}");
    }

    #[test]
    fn something_that_is_not_http_is_not_healthy() {
        let address = serve_once(Some("not http at all"));
        assert!(probe(&address).is_err());
    }

    #[test]
    fn nothing_listening_is_not_healthy() {
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .to_string(); // the listener is dropped: the port is closed
        let err = probe(&address).unwrap_err();
        assert!(err.contains("cannot connect"), "{err}");
    }

    #[test]
    fn the_address_comes_from_port_and_bind_address() {
        assert_eq!(probe_address(None, None).unwrap(), "127.0.0.1:8080");
        assert_eq!(
            probe_address(Some("9000"), Some("0.0.0.0")).unwrap(),
            "127.0.0.1:9000"
        );
        assert_eq!(
            probe_address(Some(" 9000 "), Some("::")).unwrap(),
            "127.0.0.1:9000"
        );
        assert_eq!(
            probe_address(Some("80"), Some("10.0.0.7")).unwrap(),
            "10.0.0.7:80"
        );
        assert_eq!(
            probe_address(Some("80"), Some("fe80::1")).unwrap(),
            "[fe80::1]:80"
        );
        assert_eq!(probe_address(Some(""), Some("")).unwrap(), "127.0.0.1:8080");
        assert!(probe_address(Some("http"), None)
            .unwrap_err()
            .contains("PORT"));
        assert!(probe_address(Some("70000"), None).is_err());
    }
}
