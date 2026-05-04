# loomctl

Command-line interface for controlling and monitoring the Loom cryptocurrency arbitrage daemon. This binary connects to the gRPC server running on the daemon to query status, list detected arbitrage opportunities, and manage the engine in real time.

## Overview

`loomctl` is a stateless client that communicates with the Loom daemon via gRPC. It supports both TCP and Unix socket transports, and can output results in human-readable text or JSON format. All commands are read-only or administrative (e.g., shutdown) and do not modify market state directly.

## Public API

The CLI exposes these subcommands via the daemon's gRPC service:

- `status` — Get daemon uptime, FIX connection state, ticks processed, and opportunities detected
- `mode` — Retrieve the current engine mode
- `ls` — List all active lump sums (capital parcels) and their states
- `opportunities` — Query detected arbitrage cycles with optional filtering by time window or gating status
- `graph` — Inspect graph topology (node/edge count and discovered currencies)
- `events` — Stream real-time daemon events (e.g., new opportunities, market topology changes)
- `export-ticks` — Export raw market data ticks with optional filtering by time or instrument
- `shutdown` — Gracefully shut down the daemon

## Usage

```bash
# Query daemon status (human-readable output)
loomctl status

# Get status as JSON
loomctl --json status

# List lump sums
loomctl ls

# List top 50 opportunities detected in the last hour
loomctl opportunities --limit 50

# Filter for only gated opportunities
loomctl opportunities --gated-only

# Stream events in real time
loomctl events

# Connect via TCP instead of the default Unix socket
loomctl --server http://127.0.0.1:50051 status

# Connect via Unix socket (default)
loomctl --server unix:///tmp/loom.sock status
```

## Dependencies on other Loom crates

- `loom-proto` — gRPC service definitions and message types

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
