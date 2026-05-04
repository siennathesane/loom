# loom-config

Configuration management for Loom, providing TOML-based settings for FIX connectivity, engine parameters, transport (TCP or Unix socket), and database location. The crate defines the top-level `LoomConfig` struct and handles file loading and validation.

## Overview

`loom-config` deserializes TOML configuration files into strongly-typed Rust structs. It defines configuration for four domains: FIX server connection (address, credentials, heartbeat), engine behavior (safety margin, fees, cycle check interval), gRPC server transport, and database path. A sandbox configuration example is provided for quick setup.

## Public API

- `LoomConfig` — Top-level config struct with four sections
- `FixConfig` — FIX server address, credentials, and heartbeat settings
- `EngineConfig` — Safety margin, fee basis points, cycle check interval
- `ServerConfig` — Transport type (tcp/unix) and address/socket path
- `DatabaseConfig` — SQLite database file path

## Usage

```rust
use loom_config::LoomConfig;

// Load from TOML file
let config = LoomConfig::from_file("loom.toml")?;

// Or create a sandbox example for testing
let example = LoomConfig::sandbox_example();
println!("{}", toml::to_string_pretty(&example)?);
```

## Configuration File Example

```toml
[fix]
address = "fix-public.sandbox.exchange.coinbase.com:4198"
server_name = "fix-public.sandbox.exchange.coinbase.com"
sender_comp_id = "YOUR_API_KEY"
target_comp_id = "Coinbase"
api_secret = "YOUR_API_SECRET"
passphrase = "YOUR_PASSPHRASE"
heartbeat_interval = 30

[engine]
safety_margin = 1.5
fee_bps = 60
cycle_check_interval_ms = 100

[server]
transport = "unix"
address = "/tmp/loom.sock"

[database]
path = "./loom.db"
```

## Dependencies on other Loom crates

None — `loom-config` is self-contained.

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
