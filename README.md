# FerrumKV 🦀

A lightweight, multi-threaded KV storage server written in Rust — built from scratch for systems programming practice.

## Architecture

```mermaid
%%{init: {'theme': 'neutral', 'flowchart': {'curve': 'basis'}}}%%
flowchart TB
    Client(["Client<br/>(telnet / netcat)"])

    Client -->|"TCP :6380"| Listener

    subgraph Server ["🦀 FerrumKV Server"]
        direction TB

        subgraph Network ["Network Layer"]
            Listener["TcpListener<br/>thread::spawn per connection"]
        end

        subgraph Threads ["Concurrency Layer"]
            direction LR
            T1["Thread 1<br/>handle_client"]
            T2["Thread 2<br/>handle_client"]
            T3["Thread N<br/>handle_client"]
        end

        Listener -.->|spawn| T1
        Listener -.->|spawn| T2
        Listener -.->|spawn| T3

        subgraph Handler ["Processing Layer"]
            direction LR
            Parse["Protocol::parse"] --> Exec["execute_command"] --> Fmt["format_response"]
        end

        T1 & T2 & T3 --> Parse

        subgraph Storage ["Storage Layer"]
            Engine[("KvEngine<br/>Arc‹RwLock‹HashMap››")]
        end

        Exec -->|"shared access"| Engine
    end

    Fmt -->|"response"| Client

    style Client fill:#f4a261,stroke:#e76f51,stroke-width:2px,color:#000
    style Listener fill:#457b9d,stroke:#1d3557,stroke-width:2px,color:#fff
    style T1 fill:#2a9d8f,stroke:#264653,stroke-width:2px,color:#fff
    style T2 fill:#2a9d8f,stroke:#264653,stroke-width:2px,color:#fff
    style T3 fill:#2a9d8f,stroke:#264653,stroke-width:2px,color:#fff
    style Parse fill:#a8dadc,stroke:#457b9d,stroke-width:1.5px,color:#000
    style Exec fill:#a8dadc,stroke:#457b9d,stroke-width:1.5px,color:#000
    style Fmt fill:#a8dadc,stroke:#457b9d,stroke-width:1.5px,color:#000
    style Engine fill:#e9c46a,stroke:#e76f51,stroke-width:2px,color:#000
    style Network fill:#e8f0fe,stroke:#457b9d,stroke-width:1.5px,color:#1d3557
    style Threads fill:#e6f5f0,stroke:#2a9d8f,stroke-width:1.5px,color:#264653
    style Handler fill:#f0faf8,stroke:#2a9d8f,stroke-width:1.5px,color:#264653
    style Storage fill:#fef9e7,stroke:#e9c46a,stroke-width:1.5px,color:#7c6a0a
    style Server fill:#eef2ff,stroke:#4338ca,stroke-width:3px,color:#1e1b4b
```

## Quick Start

```bash
# Build
cargo build

# Run server (listens on 127.0.0.1:6380)
cargo run

# Connect with telnet or netcat
telnet 127.0.0.1 6380
```

## Supported Commands

| Command           | Description                  | Response          |
|--------------------|------------------------------|-------------------|
| `SET key value`   | Store a key-value pair       | `OK`              |
| `GET key`         | Retrieve value by key        | value or `NULL`   |
| `DEL key`         | Delete a key                 | `OK` or `NULL`    |
| `PING`            | Health check                 | `PONG`            |

Commands are **case-insensitive**. Values can contain spaces (e.g. `SET msg hello world`).

## Roadmap

- [ ] TTL (key expiration)
- [ ] AOF persistence
- [ ] RESP protocol support
- [ ] Async I/O (tokio)

## License

MIT
