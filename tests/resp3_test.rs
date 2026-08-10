//! End-to-end tests for RESP3 typed replies (P-02).
//!
//! Boots a real server and verifies that, after a `HELLO 3` handshake,
//! commands return RESP3 types (null `_`, boolean `#t`/`#f`, map `%N`)
//! while RESP2 clients keep the classic encodings.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use ferrum_kv::network::server::{self, ServerConfig};
use ferrum_kv::network::shutdown::Shutdown;
use ferrum_kv::storage::engine::KvEngine;

struct ServerGuard {
    addr: String,
    shutdown: Shutdown,
    _thread: thread::JoinHandle<()>,
}

impl ServerGuard {
    fn shutdown(self) {
        self.shutdown.trigger();
        let _ = self._thread.join();
    }
}

fn spawn_server() -> ServerGuard {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr").to_string();
    let engine = KvEngine::new();
    let shutdown = Shutdown::new();
    let shutdown_for_thread = shutdown.clone();
    let handle = thread::spawn(move || {
        let _ = server::run_listener(
            listener,
            engine,
            shutdown_for_thread,
            ServerConfig::default(),
        );
    });
    thread::sleep(Duration::from_millis(500));
    ServerGuard {
        addr,
        shutdown,
        _thread: handle,
    }
}

fn connect(addr: &str) -> TcpStream {
    let s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    s
}

fn build_request(args: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(format!("*{}\r\n", args.len()).as_bytes());
    for arg in args {
        out.extend_from_slice(format!("${}\r\n", arg.len()).as_bytes());
        out.extend_from_slice(arg);
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// Writes a request and reads exactly one reply frame. The frame reader
/// returns as soon as the reply is complete — a read-until-timeout loop
/// would block for the full socket read timeout on every command because
/// the server keeps the connection open.
fn send(stream: &mut TcpStream, args: &[&[u8]]) -> Vec<u8> {
    stream.write_all(&build_request(args)).expect("write");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let reply = read_one_frame(stream);
        if !reply.is_empty() {
            return reply;
        }
        // Server not ready yet (startup race); retry until the deadline.
        assert!(Instant::now() < deadline, "server did not reply");
        thread::sleep(Duration::from_millis(50));
    }
}

/// Reads exactly one RESP frame of any type (line-based, length-prefixed,
/// or recursive containers) using `read_exact`. Returns an empty vec if the
/// first byte does not arrive within the socket read timeout.
fn read_one_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut out = Vec::new();
    let mut byte = [0u8; 1];
    if stream.read_exact(&mut byte).is_err() {
        return Vec::new();
    }
    out.push(byte[0]);
    match byte[0] {
        // Line-based: simple string, error, integer, null, boolean, double.
        b'+' | b'-' | b':' | b'_' | b'#' | b',' => read_until_crlf(stream, &mut out),
        // Length-prefixed bulk string (incl. null bulk `$-1`).
        b'$' => {
            let mut header = Vec::new();
            read_until_crlf(stream, &mut header);
            out.extend_from_slice(&header);
            let header_str = std::str::from_utf8(&header[..header.len() - 2]).expect("ascii len");
            let len: i64 = header_str.parse().expect("integer length");
            if len >= 0 {
                let mut body = vec![0u8; len as usize + 2];
                stream.read_exact(&mut body).expect("read bulk body");
                out.extend_from_slice(&body);
            }
        }
        // Containers: array `*N`, map `%N` (2N elements), set `~N`.
        b'*' | b'%' | b'~' => {
            let mut header = Vec::new();
            read_until_crlf(stream, &mut header);
            out.extend_from_slice(&header);
            let header_str = std::str::from_utf8(&header[..header.len() - 2]).expect("ascii len");
            let count: i64 = header_str.parse().expect("integer length");
            let elements = if byte[0] == b'%' { count * 2 } else { count };
            for _ in 0..elements {
                out.extend_from_slice(&read_one_frame(stream));
            }
        }
        other => panic!("unexpected RESP type byte {other:#x}"),
    }
    out
}

fn read_until_crlf(stream: &mut TcpStream, out: &mut Vec<u8>) {
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).expect("read byte");
        out.push(byte[0]);
        if out.len() >= 2 && out[out.len() - 2] == b'\r' && out[out.len() - 1] == b'\n' {
            return;
        }
    }
}

/// Negotiates RESP3 on a fresh connection.
fn connect_resp3(addr: &str) -> TcpStream {
    let mut s = connect(addr);
    let reply = send(&mut s, &[b"HELLO", b"3"]);
    assert!(
        String::from_utf8_lossy(&reply).contains(":3\r\n"),
        "HELLO 3 should report proto 3: {:?}",
        String::from_utf8_lossy(&reply)
    );
    s
}

#[test]
fn resp3_get_miss_returns_null_type() {
    let server = spawn_server();
    let mut s = connect_resp3(&server.addr);

    let reply = send(&mut s, &[b"GET", b"missing"]);
    assert_eq!(reply, b"_\r\n", "RESP3 GET miss must be `_`");

    server.shutdown();
}

#[test]
fn resp3_expire_returns_boolean() {
    let server = spawn_server();
    let mut s = connect_resp3(&server.addr);

    assert_eq!(send(&mut s, &[b"SET", b"k", b"v"]), b"+OK\r\n");
    // EXPIRE returns a boolean in RESP3.
    assert_eq!(send(&mut s, &[b"EXPIRE", b"k", b"100"]), b"#t\r\n");
    // Unknown key returns the false boolean.
    assert_eq!(send(&mut s, &[b"EXPIRE", b"nokey", b"100"]), b"#f\r\n");

    server.shutdown();
}

#[test]
fn resp3_config_get_returns_map() {
    let server = spawn_server();
    let mut s = connect_resp3(&server.addr);

    let reply = send(&mut s, &[b"CONFIG", b"GET", b"*"]);
    // 6 exposed parameters → RESP3 map header `%6\r\n` (vs `*12\r\n` RESP2).
    assert!(
        reply.starts_with(b"%6\r\n"),
        "RESP3 CONFIG GET * must start with %6 map header, got {:?}",
        String::from_utf8_lossy(&reply)
    );
    let text = String::from_utf8_lossy(&reply);
    assert!(
        text.contains("maxmemory-policy"),
        "map should include policy: {text}"
    );

    server.shutdown();
}

#[test]
fn resp3_incrbyfloat_returns_double() {
    let server = spawn_server();
    let mut s = connect_resp3(&server.addr);

    assert_eq!(send(&mut s, &[b"SET", b"f", b"1.5"]), b"+OK\r\n");
    // RESP3 INCRBYFLOAT → native double.
    let reply = send(&mut s, &[b"INCRBYFLOAT", b"f", b"2.25"]);
    assert_eq!(reply, b",3.75\r\n", "RESP3 INCRBYFLOAT must be a double");

    server.shutdown();
}

#[test]
fn resp3_info_returns_map() {
    let server = spawn_server();
    let mut s = connect_resp3(&server.addr);

    let reply = send(&mut s, &[b"INFO"]);
    // Four sections (server, memory, stats, keyspace) → `%4\r\n` map header.
    assert!(
        reply.starts_with(b"%4\r\n"),
        "RESP3 INFO must be a 4-pair map, got {:?}",
        String::from_utf8_lossy(&reply)
    );
    let text = String::from_utf8_lossy(&reply);
    assert!(text.contains("server"), "map should include server: {text}");
    assert!(text.contains("stats"), "map should include stats: {text}");
    assert!(
        text.contains("keyspace"),
        "map should include keyspace: {text}"
    );

    server.shutdown();
}

#[test]
fn resp2_clients_keep_classic_encodings() {
    let server = spawn_server();
    let mut s = connect(&server.addr); // no HELLO → RESP2 default

    // GET miss stays `$-1\r\n`.
    assert_eq!(send(&mut s, &[b"GET", b"missing"]), b"$-1\r\n");

    // EXPIRE stays an integer.
    assert_eq!(send(&mut s, &[b"SET", b"k", b"v"]), b"+OK\r\n");
    assert_eq!(send(&mut s, &[b"EXPIRE", b"k", b"100"]), b":1\r\n");

    // CONFIG GET * stays a flat array `*12\r\n`.
    let reply = send(&mut s, &[b"CONFIG", b"GET", b"*"]);
    assert!(
        reply.starts_with(b"*12\r\n"),
        "RESP2 CONFIG GET * must start with *12 array, got {:?}",
        String::from_utf8_lossy(&reply)
    );

    // INCRBYFLOAT stays a bulk string.
    assert_eq!(send(&mut s, &[b"SET", b"f", b"1"]), b"+OK\r\n");
    let reply = send(&mut s, &[b"INCRBYFLOAT", b"f", b"0.5"]);
    assert!(
        reply.starts_with(b"$"),
        "RESP2 INCRBYFLOAT must be a bulk string, got {:?}",
        String::from_utf8_lossy(&reply)
    );
    assert!(
        String::from_utf8_lossy(&reply).contains("1.5"),
        "RESP2 INCRBYFLOAT payload should be 1.5, got {:?}",
        String::from_utf8_lossy(&reply)
    );

    // INFO stays a single bulk string.
    let reply = send(&mut s, &[b"INFO"]);
    assert!(
        reply.starts_with(b"$"),
        "RESP2 INFO must be a bulk string, got {:?}",
        String::from_utf8_lossy(&reply)
    );
    assert!(
        String::from_utf8_lossy(&reply).contains("# Server"),
        "RESP2 INFO should contain the Server section"
    );

    server.shutdown();
}
