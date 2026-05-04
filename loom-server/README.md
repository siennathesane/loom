# loom-server

gRPC server that exposes the Loom arbitrage engine state and event stream. The server is embedded in the daemon and listens on a configurable TCP or Unix socket endpoint, allowing `loomctl` and other clients to query the engine in real time without direct access to internal state.

## Overview

`loom-server` implements the `LoomControl` gRPC service, maintaining a shared state mutex with metrics (opportunities detected, ticks processed, graph size) and a broadcast channel for event notifications. It is responsible for all client-facing communication and must be thread-safe and performant under high event rates.

## Public API

- `LoomControlService` — gRPC service handler with state management
- `start_server(TransportType, db_path)` — Bind and start the server
- `TransportType::Tcp(addr)` / `TransportType::Unix(path)` — Transport selection

The service implements these RPC methods (inherited from proto):

- `get_status()` — Return engine metrics and connection state
- `list_opportunities()` — Query opportunity log with filtering
- `list_lump_sums()` — List active capital parcels
- `stream_events()` — Server-streaming RPC for event subscription
- `get_graph_state()` — Snapshot of currency graph topology
- `export_ticks()` — Export market data with time filtering
- `shutdown()` — Initiate daemon shutdown

## Usage

The server is started by `loom-daemon` and is not intended for standalone use. In the daemon's main loop:

```rust
use loom_server::{start_server, TransportType};

let transport = TransportType::Unix("/tmp/loom.sock".into());
tokio::spawn(async move {
    if let Err(e) = start_server(transport, "/path/to/loom.db").await {
        eprintln!("Server error: {}", e);
    }
});
```

Clients connect via `loomctl` or any gRPC client library:

```bash
loomctl --server http://127.0.0.1:50051 status
```

## Dependencies on other Loom crates

- `loom-proto` — gRPC service and message type definitions
- `loom-engine` — Engine state and opportunity data
- `loom-store` — Database access for opportunity and tick queries

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
