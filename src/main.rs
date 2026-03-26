mod error;
mod network;
mod protocol;
mod storage;

use storage::engine::KvEngine;

fn main() {
    let addr = "127.0.0.1:6380";
    let engine = KvEngine::new();

    println!("[INFO] FerrumKV v0.1.0 starting...");
    network::server::start(addr, engine);
}
