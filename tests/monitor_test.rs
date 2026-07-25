//! End-to-end tests for the `MONITOR` command.
//!
//! Boots a real server, connects two clients, puts one in MONITOR mode, and
//! asserts that the monitoring client receives every command processed by
//! the other connection.

use std::io::{BufRead, BufReader, Read, Write};
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
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).expect("read");
    buf[..n].to_vec()
}

#[test]
fn monitor_receives_commands_from_other_client() {
    let server = spawn_server();

    // Client A: enter MONITOR mode.
    let mut ma = connect(&server.addr);
    assert_eq!(send(&mut ma, &[b"MONITOR"]), b"+OK\r\n");

    // Client B: issue regular commands.
    let mut mb = connect(&server.addr);
    assert_eq!(send(&mut mb, &[b"SET", b"x", b"1"]), b"+OK\r\n");
    assert_eq!(send(&mut mb, &[b"SET", b"y", b"2"]), b"+OK\r\n");

    // Client A (monitor) must have received both commands.
    let mut reader = BufReader::new(ma);
    let mut line1 = String::new();
    let mut line2 = String::new();
    reader.read_line(&mut line1).expect("first monitor line");
    reader.read_line(&mut line2).expect("second monitor line");

    assert!(
        line1.contains("SET") && line1.contains("x") && line1.contains("1"),
        "first monitored command should be SET x 1, got: {line1}"
    );
    assert!(
        line2.contains("SET") && line2.contains("y") && line2.contains("2"),
        "second monitored command should be SET y 2, got: {line2}"
    );
    // Redis MONITOR output: +timestamp [addr] "cmd" "arg"...
    assert!(
        line1.starts_with('+'),
        "monitor lines start with + (simple string)"
    );
    assert!(
        line2.starts_with('+'),
        "monitor lines start with + (simple string)"
    );

    server.shutdown();
}

#[test]
fn monitor_does_not_see_its_own_command() {
    let server = spawn_server();

    // Client A: enter MONITOR mode.
    let mut ma = connect(&server.addr);
    assert_eq!(send(&mut ma, &[b"MONITOR"]), b"+OK\r\n");

    // Client B: issue a command.
    let mut mb = connect(&server.addr);
    assert_eq!(send(&mut mb, &[b"SET", b"k", b"v"]), b"+OK\r\n");

    // Client A must receive B's SET but not its own MONITOR.
    let mut reader = BufReader::new(ma);
    let mut line = String::new();
    reader.read_line(&mut line).expect("monitor line");
    assert!(line.contains("SET"), "monitor must see the SET command");
    assert!(
        !line.contains("MONITOR"),
        "monitor must NOT see its own MONITOR"
    );

    server.shutdown();
}
