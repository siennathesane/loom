# loom-proto

Protocol buffer definitions for the Loom gRPC API. This crate contains all `.proto` message definitions and is the single source of truth for daemon and client communication. Rust code is generated at build time via `tonic-build`.

## Overview

`loom-proto` defines the gRPC service interface (`LoomControl`) and all request/response message types used by `loomctl` and `loom-server`. The protobuf schemas are compiled to Rust using Prost and Tonic, enabling type-safe, efficient serialization for status queries, event streaming, and opportunity listing.

## Public API

The crate re-exports all generated types. Key service methods (defined in proto):

- `get_status(StatusRequest) -> StatusResponse` — Engine metrics and connection state
- `list_opportunities(ListOpportunitiesRequest) -> ListOpportunitiesResponse` — Query detected cycles
- `list_lump_sums(ListLumpSumsRequest) -> ListLumpSumsResponse` — Capital parcel inventory
- `stream_events(EventStreamRequest) -> Stream<Event>` — Real-time event notifications
- `get_graph_state(GraphStateRequest) -> GraphStateResponse` — Topology snapshot
- `export_ticks(ExportTicksRequest) -> ExportTicksResponse` — Market data export

## Usage

This crate is consumed by `loomctl` (client) and `loom-server` (server). End users do not use `loom-proto` directly; they interact via the CLI or gRPC calls.

```rust
use loom_proto::{loom_control_client::LoomControlClient, StatusRequest};

// Client side
let mut client = LoomControlClient::connect("http://127.0.0.1:50051").await?;
let response = client.get_status(StatusRequest {}).await?;
println!("Mode: {}", response.into_inner().mode);
```

## Build

The `.proto` files are located in `proto/` and are compiled during the Cargo build process. The generated Rust code is included in the binary and does not need to be manually edited.

## Dependencies on other Loom crates

None — `loom-proto` is self-contained.

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
