# loom-daemon

Loom arbitrage daemon binary. This is the main entry point that orchestrates the FIX protocol client, the arbitrage detection engine, the graph topology manager, and the gRPC control server. It runs continuously, monitoring market data and executing arbitrage cycles.

## Overview

`loom-daemon` is a long-running binary that binds together all Loom components into a cohesive trading system. On startup, it loads configuration, connects to Coinbase's FIX server, discovers available trading pairs, constructs the currency graph, starts the arbitrage event loop, and launches the gRPC control server. It handles FIX session management, market data ingestion, cycle detection, and graceful shutdown.

## Public API

This is a binary crate, not a library. No public API.

## Usage

Generate an example configuration file:

```bash
loom-daemon --example-config > loom.toml
# Edit loom.toml with your Coinbase API credentials
```

Run the daemon:

```bash
loom-daemon --config loom.toml
```

In another terminal, use `loomctl` to query the daemon:

```bash
loomctl status
loomctl opportunities --limit 10
loomctl events
```

## Startup Sequence

1. Load TOML configuration (FIX credentials, engine parameters, database path)
2. Create a placeholder graph (currencies discovered dynamically)
3. Initialize the arbitrage event loop and SQLite database
4. Start the gRPC control server (Unix socket or TCP)
5. Connect to Coinbase FIX server with TLS
6. Send Logon (HMAC-signed) and request SecurityList to discover all trading pairs
7. Recreate graph with real currencies and subscribe to market data for all pairs
8. Enter main event loop: process FIX messages, check for arbitrage cycles, handle heartbeats
9. On shutdown, send FIX Logout and flush all buffered data

## Dependencies on other Loom crates

- `loom-config` — Load and parse configuration
- `loom-fix` — FIX 5.0 protocol client and session management
- `loom-graph` — Currency graph and cycle detection
- `loom-engine` — Arbitrage event loop and cycle detection logic
- `loom-server` — gRPC control interface
- `loom-store` — Database persistence

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
