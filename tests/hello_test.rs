//! End-to-end tests for the `HELLO` command.
//!
//! Boots a real server and tests RESP3 protocol handshake negotiation.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

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

fn send(stream: &mut TcpStream, args: &[&[u8]]) -> Vec<u8> {
    stream.write_all(&build_request(args)).expect("write");
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).expect("read");
    buf[..n].to_vec()
}

#[test]
fn hello_2_returns_server_metadata_in_resp2() {
    let server = spawn_server();
    let mut s = connect(&server.addr);

    let reply = send(&mut s, &[b"HELLO", b"2"]);
    let text = String::from_utf8_lossy(&reply);

    // Should contain key-value pairs: server, version, proto, mode
    assert!(text.contains("server"), "missing server key: {text}");
    assert!(text.contains("ferrum"), "missing ferrum value: {text}");
    assert!(text.contains("proto"), "missing proto key: {text}");
    assert!(text.contains("version"), "missing version key: {text}");
    assert!(text.contains("mode"), "missing mode key: {text}");
    assert!(text.contains("standalone"), "missing standalone: {text}");
    // proto should be 2 since we asked for RESP2
    assert!(text.contains(":2\r\n"), "proto should be 2: {text}");

    // After HELLO 2, subsequent commands should still work in RESP2
    let reply = send(&mut s, &[b"SET", b"k", b"v"]);
    assert_eq!(
        String::from_utf8_lossy(&reply).trim(),
        "+OK",
        "SET should work after HELLO 2"
    );

    server.shutdown();
}

#[test]
fn hello_3_returns_resp3_proto_value() {
    let server = spawn_server();
    let mut s = connect(&server.addr);

    let reply = send(&mut s, &[b"HELLO", b"3"]);
    let text = String::from_utf8_lossy(&reply);

    // proto should be 3 for RESP3 handshake
    assert!(text.contains(":3\r\n"), "proto should be 3: {text}");
    assert!(
        text.contains("ferrum"),
        "should include server name: {text}"
    );

    server.shutdown();
}

#[test]
fn hello_without_args_queries_current_protocol() {
    let server = spawn_server();
    let mut s = connect(&server.addr);

    // Default is RESP2; HELLO without args returns current proto (2)
    let reply = send(&mut s, &[b"HELLO"]);
    let text = String::from_utf8_lossy(&reply);
    assert!(text.contains(":2\r\n"), "default proto should be 2: {text}");

    server.shutdown();
}
