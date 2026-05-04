# Loom - Cryptocurrency Arbitrage Engine

Loom is a high-performance arbitrage detection engine for cryptocurrency exchanges. It connects to Coinbase via the FIX protocol, maintains a real-time graph of trading pairs, and detects profitable arbitrage cycles using gravitational gating.

## Table of Contents

- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Setup](#setup)
- [Running the Daemon](#running-the-daemon)
- [Using the CLI (loomctl)](#using-the-cli-loomctl)
- [Configuration Options](#configuration-options)
- [Development Status](#development-status)
- [Testing](#testing)
- [Troubleshooting](#troubleshooting)
- [Documentation](#documentation)
- [License](#license)
- [Contributing](#contributing)

## Architecture

- **loom-daemon**: Main binary that connects to Coinbase FIX and runs the arbitrage detection engine
- **loomctl**: CLI tool for interacting with the daemon via gRPC
- **loom-engine**: Core arbitrage detection engine with gravitational gating
- **loom-graph**: Graph data structure for trading pairs and exchange rates
- **loom-fix**: FIX protocol client for Coinbase
- **loom-server**: gRPC server for daemon control
- **loom-store**: SQLite-backed opportunity logging
- **loom-config**: Configuration management

For a deeper system overview, data flow, and design decisions, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Prerequisites

- Rust 1.75 or later
- Coinbase API credentials (sandbox or production)
  - Get sandbox credentials at: https://public.sandbox.exchange.coinbase.com/settings/api

## Setup

### 1. Clone and Build

```bash
git clone <repository-url>
cd loom
cargo build --release
```

### 2. Configure

Copy the example configuration and fill in your Coinbase API credentials:

```bash
cp loom.example.toml loom.toml
```

Edit `loom.toml` and replace the placeholders:
- `sender_comp_id`: Your Coinbase API key
- `api_secret`: Your Coinbase API secret
- `passphrase`: Your API passphrase

### 3. Generate Example Config (Optional)

You can generate an example configuration programmatically:

```bash
./target/release/loom-daemon --example-config > loom.toml
```

## Running the Daemon

Start the daemon with your configuration:

```bash
./target/release/loom-daemon --config loom.toml
```

The daemon will:
1. Load the configuration
2. Initialize the trading pair graph (BTC/USD, ETH/USD, ETH/BTC)
3. Create a SQLite database for opportunity logging
4. Start the gRPC server (default: Unix socket at `/tmp/loom.sock`)
5. Connect to Coinbase FIX server
6. Begin processing market data and detecting arbitrage cycles

## Using the CLI (loomctl)

Once the daemon is running, you can interact with it using `loomctl`:

```bash
# Get daemon status
./target/release/loomctl status

# List all trading pairs
./target/release/loomctl pairs

# Query recent arbitrage opportunities
./target/release/loomctl opportunities --limit 10

# Get specific opportunity details
./target/release/loomctl opportunity <id>
```

## Configuration Options

### FIX Connection
- `address`: FIX server address (sandbox: `fix-public.sandbox.exchange.coinbase.com:4198`)
- `server_name`: TLS server name
- `sender_comp_id`: Your API key
- `target_comp_id`: Usually "Coinbase"
- `api_secret`: Your API secret for HMAC signing
- `passphrase`: Your API passphrase
- `heartbeat_interval`: Heartbeat interval in seconds (default: 30)

### Engine
- `safety_margin`: Safety margin for gravitational gating (default: 1.5 = 150%)
- `fee_bps`: Trading fee in basis points (default: 60 = 0.6%)
- `cycle_check_interval_ms`: How often to check for cycles (default: 100ms)

### Server
- `transport`: "unix" or "tcp"
- `address`: Unix socket path or TCP address:port

### Database
- `path`: Path to SQLite database file (default: `./loom.db`)

## Development Status

### ✅ Phase 1: FIX Protocol Foundation (Complete)
- Hand-rolled FIX 5.0 SP2 implementation
- HMAC-SHA256 authentication
- TLS transport with session management
- Zero-allocation hot paths
- Sub-microsecond serialization (100x faster than target)
- 27K msg/sec market data throughput
- **Test Coverage**: 63+ tests passing (protocol compliance + integration)

### ✅ Phase 2: Graph Engine (Complete)
- Adjacency matrix with SOA layout (cache-optimized)
- Bellman-Ford negative cycle detection
- Dijkstra shortest paths for gravitational potential
- Snapshot capture for validation
- rust_decimal for exact financial arithmetic
- **Performance**: 1.76µs Bellman-Ford (556x headroom vs 1ms target)
- **Test Coverage**: 75 tests passing

### ✅ Phase 3: Listen Mode Integration (Complete)
- Event loop with FIX integration
- Dynamic currency discovery via SecurityListRequest
- Graph rebuilding after SecurityList response
- Market data subscriptions for all discovered pairs
- gRPC server with Unix socket transport
- CLI binary (loomctl) with all commands
- Event streaming and currency tracking
- **Test Coverage**: 61 integration tests passing

### 🚧 Phase 4: Active Mode (In Progress)
- Order execution logic
- Risk management
- State machine implementation
- Lump sum lifecycle management

### 📋 Phase 5: Production Readiness (Planned)
- Deployment automation
- Performance optimization
- Security audit
- 24-hour stress testing

## Testing

Run all tests:

```bash
cargo test --workspace
```

Run tests for a specific crate:

```bash
cargo test -p loom-engine
cargo test -p loom-graph
cargo test -p loom-fix
```

## Troubleshooting

### Connection Issues
- Verify your API credentials are correct
- Check that you're using the correct FIX server address (sandbox vs production)
- Ensure your API key has the necessary permissions

### Unix Socket Issues
- If the daemon fails to start with "Address already in use", remove the socket file:
  ```bash
  rm /tmp/loom.sock
  ```

### Database Issues
- The database file will be created automatically on first run
- If you encounter corruption, delete the database file and restart:
  ```bash
  rm loom.db
  ```

## Documentation

Comprehensive documentation is available in the following files:

### Technical Reports
- **[docs/numerical_precision_decision.md](docs/numerical_precision_decision.md)** - Precision strategy analysis and recommendations
- **[docs/benchmark_results.md](docs/benchmark_results.md)** - Performance benchmark results (Phase 1)
- **[docs/phase1_performance_report.md](docs/phase1_performance_report.md)** - FIX protocol performance validation
- **[docs/phase2_integration_review.md](docs/phase2_integration_review.md)** - Graph engine API integration analysis
- **[docs/phase3_integration_plan.md](docs/phase3_integration_plan.md)** - Listen mode architecture and implementation

### Test Documentation
- **[loom-harness/TEST_COVERAGE.md](loom-harness/TEST_COVERAGE.md)** - FIX compliance test coverage summary
- **[loom-harness/tests/COINBASE_COMPLIANCE.md](loom-harness/tests/COINBASE_COMPLIANCE.md)** - Coinbase FIX API compliance validation

### Component READMEs

Each workspace crate has its own README:
- **[loom-daemon](loom-daemon/README.md)** — Main binary that orchestrates everything
- **[loomctl](loomctl/README.md)** — gRPC CLI client
- **[loom-engine](loom-engine/README.md)** — Arbitrage detection engine
- **[loom-graph](loom-graph/README.md)** — Trading-pair graph and cycle detection
- **[loom-fix](loom-fix/README.md)** — FIX 5.0 SP2 protocol client
- **[loom-server](loom-server/README.md)** — gRPC service implementation
- **[loom-store](loom-store/README.md)** — SQLite-backed opportunity logging
- **[loom-config](loom-config/README.md)** — TOML configuration loader
- **[loom-proto](loom-proto/README.md)** — Protocol buffer definitions
- **[loom-harness](loom-harness/README.md)** — FIX protocol test harness

## Performance

Loom achieves exceptional performance across all critical paths:

| Component | Target | Actual | Status |
|-----------|--------|--------|--------|
| FIX Serialization (p99) | < 100μs | **< 1μs** | ✅ 100x better |
| Market Data Parsing | > 10K msg/sec | **27K msg/sec** | ✅ 2.7x better |
| Bellman-Ford (p99) | < 1ms | **1.76μs** | ✅ 556x better |
| End-to-End Latency | < 100μs | **20-60μs** | ✅ Within budget |

## License

Loom is licensed under the **GNU Affero General Public License v3.0 or later** (AGPL-3.0-or-later). See [LICENSE](LICENSE) for the full text.

The AGPL-3.0 ensures that any modified version run as a network service must make its source available to users of that service. If this license does not fit your use case, please contact the author.

## Contributing

Issues and pull requests are welcome at [github.com/siennathesane/loom](https://github.com/siennathesane/loom). See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, code style, and contribution workflow.
