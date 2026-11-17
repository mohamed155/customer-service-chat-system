//! `server healthcheck`: the container health check (MOH-81).
//!
//! The backend image is `debian:bookworm-slim` and has no `curl`, so it carries its own probe: the
//! same binary asks its own `/ready` endpoint and exits 0 when the answer is `200`, 1 otherwise.
//! Docker and Compose run it as `server healthcheck`, so a container is "healthy" only while
//! PostgreSQL, Redis and object storage answer and the process is not shutting down, not merely
//! while it is running.
//!
//! It reads only `PORT`, `BIND_ADDRESS` and `READY_PROBE_TIMEOUT_MS` (the listener's and the
//! readiness probe's own settings) and none of the secrets,
//! so it works with the environment the container already has. It makes one plain HTTP/1.1 request
//! over a TCP socket with short timeouts and needs no HTTP client.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// What `/ready` is given on top of its own probe budget (`READY_PROBE_TIMEOUT_MS`): connecting,
/// serving and answering. The checks run at once, so the budget is one probe timeout, not one per
/// dependency.
const READ_MARGIN: Duration = Duration::from_secs(3);
/// `READY_PROBE_TIMEOUT_MS` when it is not set (the server's default).
const DEFAULT_READY_PROBE_MS: u64 = 2_000;
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

/// How long the whole probe may take: the server's readiness budget plus a margin. An unset or
/// unreadable `READY_PROBE_TIMEOUT_MS` means the server's own default.
pub fn read_timeout(ready_probe_ms: Option<&str>) -> Duration {
    let ms = ready_probe_ms
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .unwrap_or(DEFAULT_READY_PROBE_MS);
    Duration::from_millis(ms) + READ_MARGIN
}

/// Ask `address` for `/ready`. `Ok` only for a `200`. Every address the name resolves to is tried
/// in turn (`localhost` can resolve to `::1` first while the server listens on IPv4 only). The
/// whole probe, connecting and reading, takes at most `budget`.
pub fn probe(address: &str, budget: Duration) -> Result<(), String> {
    let targets: Vec<SocketAddr> = address
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {address}: {e}"))?
        .collect();
    probe_any(address, &targets, budget)
}

/// [`probe`] over already resolved addresses: the first that accepts a connection is asked. All
/// connection attempts share one connect allowance ([`CONNECT_TIMEOUT`], within `budget`), so
/// several unreachable addresses cannot add up past it; the read gets what is left of `budget`.
pub fn probe_any(address: &str, targets: &[SocketAddr], budget: Duration) -> Result<(), String> {
    let started = Instant::now();
    let connect_until = started + CONNECT_TIMEOUT.min(budget);
    let mut last_error = format!("no address for {address}");
    for target in targets {
        let remaining = connect_until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            last_error = format!("no connection to {address} within {CONNECT_TIMEOUT:?}");
            break;
        }
        match TcpStream::connect_timeout(target, remaining) {
            Ok(stream) => {
                let left = budget
                    .saturating_sub(started.elapsed())
                    .max(Duration::from_millis(1));
                return ask(address, stream, left);
            }
            Err(e) => last_error = format!("cannot connect to {target}: {e}"),
        }
    }
    Err(last_error)
}

fn ask(address: &str, mut stream: TcpStream, read_timeout: Duration) -> Result<(), String> {
    stream
        .set_read_timeout(Some(read_timeout))
        .and_then(|_| stream.set_write_timeout(Some(read_timeout)))
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
    let timeout = read_timeout(std::env::var("READY_PROBE_TIMEOUT_MS").ok().as_deref());
    match probe(&address, timeout) {
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
        assert_eq!(probe(&address, Duration::from_secs(2)), Ok(()));
    }

    #[test]
    fn a_503_is_not_healthy_and_says_so() {
        let address = serve_once(Some("HTTP/1.1 503 Service Unavailable\r\n\r\n{}"));
        let err = probe(&address, Duration::from_secs(2)).unwrap_err();
        assert!(err.contains("503"), "{err}");
    }

    #[test]
    fn something_that_is_not_http_is_not_healthy() {
        let address = serve_once(Some("not http at all"));
        assert!(probe(&address, Duration::from_secs(2)).is_err());
    }

    #[test]
    fn nothing_listening_is_not_healthy() {
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .to_string(); // the listener is dropped: the port is closed
        let err = probe(&address, Duration::from_secs(2)).unwrap_err();
        assert!(err.contains("cannot connect"), "{err}");
    }

    #[test]
    fn the_next_address_is_tried_when_the_first_refuses() {
        let closed: SocketAddr = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let live = serve_once(Some("HTTP/1.1 200 OK\r\n\r\n{}"));
        let live_addr: SocketAddr = live.parse().unwrap();
        assert_eq!(
            probe_any("localhost", &[closed, live_addr], Duration::from_secs(2)),
            Ok(())
        );
        let err = probe_any("localhost", &[closed], Duration::from_secs(2)).unwrap_err();
        assert!(err.contains("cannot connect"), "{err}");
        assert!(probe_any("localhost", &[], Duration::from_secs(2))
            .unwrap_err()
            .contains("no address"));
    }

    #[test]
    fn a_silent_server_is_given_up_on_within_the_budget() {
        // Accepts and reads the request but never answers.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf);
            std::thread::sleep(Duration::from_secs(10));
        });
        let started = Instant::now();
        let err = probe(&address, Duration::from_millis(600)).unwrap_err();
        assert!(err.contains("no answer"), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn attempts_on_several_refusing_addresses_share_one_connect_allowance() {
        let closed = |_| -> SocketAddr {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let targets: Vec<SocketAddr> = (0..3).map(closed).collect();
        let started = Instant::now();
        assert!(probe_any("localhost", &targets, Duration::from_secs(5)).is_err());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "refusals are immediate"
        );
        // With no budget left nothing is attempted and the error says so.
        let err = probe_any("localhost", &targets, Duration::ZERO).unwrap_err();
        assert!(err.contains("no connection"), "{err}");
    }

    #[test]
    fn the_read_timeout_is_the_readiness_budget_plus_a_margin() {
        assert_eq!(read_timeout(None), Duration::from_secs(5));
        assert_eq!(read_timeout(Some("2000")), Duration::from_secs(5));
        assert_eq!(read_timeout(Some(" 10000 ")), Duration::from_secs(13));
        assert_eq!(
            read_timeout(Some("soon")),
            Duration::from_secs(5),
            "unreadable: the default"
        );
        assert_eq!(read_timeout(Some("0")), Duration::from_secs(5));
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
